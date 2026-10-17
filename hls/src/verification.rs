//! Saved-source batch analysis for command-line clients. Separate from dirty
//! editor sessions: stdin requests are serialized and responses are barriers.
//! Cache entries are complete compilation units, including unsuccessful ones.
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use hemtt_common::config::ProjectConfig;
use hemtt_workspace::{
    SourceDatabase, WorkspacePath,
    addons::Addon,
    reporting::{Codes, Severity, WorkspaceFiles},
};
use serde_json::{Value, json};
use tower_lsp::lsp_types::WorkspaceFolder;
use url::Url;

use crate::workspace::EditorWorkspace;

#[derive(Default)]
struct Session {
    root: PathBuf,
    inputs: BTreeMap<String, Vec<u8>>,
    cache: HashMap<String, Unit>,
    database: Option<Arc<hemtt_sqf::parser::database::Database>>,
    workspace: Option<EditorWorkspace>,
    sources: SourceDatabase,
    revision: u64,
}

#[derive(Clone)]
struct Unit {
    result: Value,
    dependencies: Vec<String>,
}

fn dependency_key(path: &WorkspacePath) -> String {
    if path.is_include() {
        format!("include{}", path.as_str())
    } else {
        path.as_str().trim_start_matches('/').to_string()
    }
}

// Byte comparison is intentional. Timestamps and filesystem events alone are
// insufficient for verification (restored mtimes, missed events, new includes).
fn inventory(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    fn walk(
        root: &Path,
        directory: &Path,
        files: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), String> {
        if !directory.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if matches!(entry.file_name().to_str(), Some(".git" | ".hemttout")) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Source symlink requires a fresh physical snapshot: {}",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                walk(root, &path, files)?;
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    std::fs::read(&path).map_err(|e| e.to_string())?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

fn findings(codes: Codes, sources: &SourceDatabase) -> (Vec<Value>, bool) {
    let files = WorkspaceFiles::with_sources(sources.clone());
    let mut failed = false;
    let mut results = Vec::new();
    for code in codes {
        failed |= code.severity() == Severity::Error;
        let Some(diag) = code.diagnostic() else {
            results.push(json!({"code":code.ident(), "severity":format!("{:?}",code.severity()), "message":code.message()}));
            continue;
        };
        let mut locations: Vec<Value> = diag.to_lsp(&files).into_iter().map(|(path, diagnostic)| json!({"path":path.as_str(),"include":path.is_include(),"diagnostic":diagnostic})).collect();
        locations.sort_by_key(Value::to_string);
        results.push(json!({"code":diag.code,"severity":format!("{:?}",diag.severity),"message":diag.message,"notes":diag.notes,"help":diag.help,"suggestions":diag.suggestions,"link":diag.link,"locations":locations}));
    }
    results.sort_by_key(Value::to_string);
    (results, failed)
}

fn check(
    path: &WorkspacePath,
    addon: &Arc<Addon>,
    project: &ProjectConfig,
    database: &Arc<hemtt_sqf::parser::database::Database>,
    sources: &SourceDatabase,
) -> Unit {
    let sqf = hemtt_sqf::is_compilation_unit(path);
    let (codes, dependencies, preprocessed, parsed) =
        match hemtt_preprocessor::Processor::run_with_sources(path, project.preprocessor(), sources)
        {
            Ok(processed) => {
                let dependencies = processed
                    .included_files()
                    .iter()
                    .map(dependency_key)
                    .collect();
                if sqf {
                    let checked = hemtt_sqf::check::check_with_sources(
                        &processed,
                        Some(project),
                        addon,
                        path,
                        database.clone(),
                        None,
                        sources,
                    );
                    (
                        checked.codes,
                        dependencies,
                        true,
                        checked.statements.is_some(),
                    )
                } else {
                    let checked = hemtt_config::check::check_with_sources(
                        &processed,
                        Some(project),
                        path,
                        sources,
                    );
                    (checked.codes, dependencies, true, checked.config.is_some())
                }
            }
            Err((dependencies, hemtt_preprocessor::Error::Code(code))) => (
                vec![code],
                dependencies.iter().map(dependency_key).collect(),
                false,
                false,
            ),
            Err((_, error)) => {
                return Unit {
                    result: json!({"path":path.as_str(),"kind":if sqf {"sqf"} else {"config"},"infrastructureError":error.to_string()}),
                    dependencies: Vec::new(),
                };
            }
        };
    let (diagnostics, failed) = findings(codes, sources);
    Unit {
        result: json!({"path":path.as_str(),"kind":if sqf {"sqf"} else {"config"},"preprocessed":preprocessed,"parsed":parsed,"failed":failed,"diagnostics":diagnostics}),
        dependencies,
    }
}

impl Session {
    fn verify(&mut self, request: &Value) -> Result<Value, String> {
        // Diagnostic failures are reusable; transient infrastructure failures
        // must be attempted again even when saved source bytes are unchanged.
        self.cache
            .retain(|_, unit| unit.result.get("infrastructureError").is_none());
        let root = PathBuf::from(
            request["workspace"]
                .as_str()
                .ok_or("workspace is required")?,
        );
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let before = inventory(&root)?;
        let changed: Vec<_> = before
            .keys()
            .chain(self.inputs.keys())
            .filter(|p| before.get(*p) != self.inputs.get(*p))
            .cloned()
            .collect();
        let reset = self.root != root
            || request["reset"].as_bool().unwrap_or(false)
            || self.inputs.keys().ne(before.keys())
            || changed.iter().any(|p| {
                ![".sqf", ".hpp", ".h", ".inc"]
                    .iter()
                    .any(|extension| p.to_ascii_lowercase().ends_with(extension))
            });
        if reset {
            self.cache.clear();
            self.database = None;
            self.workspace = None;
        } else if !changed.is_empty() {
            self.cache.retain(|path, unit| {
                !changed.iter().any(|p| {
                    path.trim_start_matches('/').eq_ignore_ascii_case(p)
                        || unit
                            .dependencies
                            .iter()
                            .any(|d| d.trim_start_matches('/').eq_ignore_ascii_case(p))
                })
            });
        }
        if reset {
            self.sources = SourceDatabase::new();
        } else {
            self.sources.invalidate_matching(|path| {
                changed
                    .iter()
                    .any(|p| dependency_key(path).eq_ignore_ascii_case(p))
            });
        }
        if self.workspace.is_none() {
            self.workspace = Some(
                EditorWorkspace::new(&WorkspaceFolder {
                    uri: Url::from_directory_path(&root).map_err(|()| "invalid workspace URL")?,
                    name: "verification".into(),
                })
                .ok_or("Cannot open analysis workspace")?,
            );
        }
        let workspace = self.workspace.as_ref().ok_or("Missing workspace")?;
        // Pin each effective saved source once at this serialized request
        // boundary. Includes otherwise reread the same disk header for every
        // consumer even when its raw tokens are already memoized. VFS reads
        // preserve layer precedence; the final content inventory remains the
        // authority for both caching and completion.
        self.revision += 1;
        for name in if reset {
            before.keys().collect::<Vec<_>>()
        } else {
            changed.iter().collect()
        } {
            let virtual_name = name.strip_prefix("include/").unwrap_or(name);
            let path = workspace
                .root()
                .join(virtual_name)
                .map_err(|e| e.to_string())?;
            let id = self.sources.file_id(&path);
            match path.read_to_string() {
                Ok(content) => self.sources.set_overlay(id, content, self.revision),
                Err(_) => self.sources.clear_overlay(id),
            }
        }
        // Invalid project/rule configuration is an infrastructure failure, never
        // silently defaulted. Config compilation failures do not stop SQF checks.
        let project = ProjectConfig::from_file(&root.join(".hemtt/project.toml"))
            .map_err(|e| e.to_string())?;
        if self.database.is_none() {
            self.database = Some(Arc::new(
                hemtt_sqf::parser::database::Database::a3_with_workspace(workspace.root(), false)
                    .map_err(|e| e.to_string())?,
            ));
        }
        let database = self.database.as_ref().ok_or("Missing command database")?;
        let mut units = Vec::new();
        for addon in Addon::scan(&root).map_err(|e| e.to_string())? {
            let addon = Arc::new(addon);
            let mut files = hemtt_config::files::checkable(workspace.root(), &addon)
                .map_err(|e| e.to_string())?;
            files.extend(
                workspace
                    .root()
                    .join(addon.folder())
                    .map_err(|e| e.to_string())?
                    .walk_dir()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(hemtt_sqf::is_compilation_unit),
            );
            for path in files {
                units.push((path, addon.clone()));
            }
        }
        units.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        units.dedup_by(|a, b| a.0 == b.0);
        let pending: Vec<_> = units
            .iter()
            .filter(|(path, _)| !self.cache.contains_key(path.as_str()))
            .cloned()
            .collect();
        let count = pending.len();
        let queue = Mutex::new(pending.into_iter());
        let completed = Mutex::new(Vec::new());
        let sources = &self.sources;
        std::thread::scope(|scope| {
            for _ in 0..std::thread::available_parallelism().map_or(1, |n| n.get().min(8)) {
                let queue = &queue;
                let completed = &completed;
                let project = &project;
                scope.spawn(move || {
                    loop {
                        let Some((path, addon)) = queue.lock().expect("queue").next() else {
                            break;
                        };
                        let unit = check(&path, &addon, project, database, sources);
                        completed
                            .lock()
                            .expect("completed")
                            .push((path.as_str().to_string(), unit));
                    }
                });
            }
        });
        for (path, unit) in completed.into_inner().map_err(|e| e.to_string())? {
            self.cache.insert(path, unit);
        }
        // A moving workspace cannot be cached or reported as a verified snapshot.
        if before != inventory(&root)? {
            self.cache.clear();
            return Err("Sources changed during analysis; retry on a stable snapshot".into());
        }
        self.root = root;
        self.inputs = before;
        let results: Vec<_> = units
            .iter()
            .filter_map(|(path, _)| self.cache.get(path.as_str()).map(|u| u.result.clone()))
            .collect();
        let complete = !results.is_empty()
            && results.len() == units.len()
            && results
                .iter()
                .all(|u| u.get("infrastructureError").is_none());
        Ok(
            json!({"schema":1,"completed":complete,"checked":count,"reused":results.len()-count,"units":results,"performance":hemtt_common::performance::snapshot()}),
        )
    }
}

pub fn serve() {
    let mut session = Session::default();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let result = line
            .map_err(|e| e.to_string())
            .and_then(|line| serde_json::from_str::<Value>(&line).map_err(|e| e.to_string()))
            .and_then(|request| session.verify(&request));
        let response = match result {
            Ok(result) => json!({"result":result}),
            Err(error) => {
                session = Session::default();
                json!({"error":error})
            }
        };
        if writeln!(stdout, "{response}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edited_headers_recheck_transitive_consumers_and_preserve_unrelated_units() {
        let root = std::env::temp_dir().join(format!("hemtt-header-cache-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".hemtt")).expect("directory");
        std::fs::create_dir_all(root.join("addons/test")).expect("addon");
        for (name, text) in [
            (
                ".hemtt/project.toml",
                "name='test'\nprefix='test'\n[version]\nmajor=1\nminor=0\npatch=0\n",
            ),
            ("addons/test/$PBOPREFIX$", "test"),
            ("addons/test/config.cpp", "class CfgPatches {};"),
            ("addons/test/a.sqf", "#include \"outer.hpp\"\nhint VALUE;"),
            ("addons/test/b.sqf", "hint 'unrelated';"),
            ("addons/test/outer.hpp", "#include \"inner.inc\""),
            ("addons/test/inner.inc", "#define VALUE 'original'"),
        ] {
            std::fs::write(root.join(name), text).expect("fixture");
        }
        let request = json!({"workspace":root});
        let mut session = Session::default();
        assert_eq!(session.verify(&request).expect("initial")["checked"], 3);
        for text in [
            "#define VALUE 42",
            "\u{feff}#define VALUE 'bad encoding'",
            "#include \"missing.hpp\"",
            "#define VALUE 'repaired'",
        ] {
            std::fs::write(root.join("addons/test/inner.inc"), text).expect("header edit");
            let incremental = session.verify(&request).expect("incremental");
            assert_eq!(incremental["checked"], 1);
            assert_eq!(incremental["reused"], 2);
            assert_eq!(
                incremental["units"],
                Session::default().verify(&request).expect("fresh")["units"]
            );
        }
        std::fs::write(
            root.join("addons/test/inner.inc"),
            "#include \"missing.hpp\"",
        )
        .expect("missing include");
        session.verify(&request).expect("missing");
        std::fs::write(root.join("addons/test/missing.hpp"), "#define VALUE 'new'")
            .expect("repair missing");
        assert_eq!(
            session.verify(&request).expect("new include")["units"],
            Session::default().verify(&request).expect("fresh include")["units"]
        );
        std::fs::write(
            root.join(".hemtt/project.toml"),
            "name='changed'\nprefix='test'\n[version]\nmajor=1\nminor=0\npatch=0\n",
        )
        .expect("configuration");
        assert_eq!(
            session.verify(&request).expect("configuration reset")["checked"],
            3
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }
    #[test]
    fn content_cache_invalidates_sources_includes_configuration_and_inventory() {
        let root = std::env::temp_dir().join(format!("hemtt-verification-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".hemtt")).expect("directory");
        std::fs::create_dir_all(root.join("addons/test")).expect("addon");
        std::fs::write(
            root.join(".hemtt/project.toml"),
            "name='test'\nprefix='test'\n[version]\nmajor=1\nminor=0\npatch=0\n",
        )
        .expect("project");
        std::fs::write(root.join("addons/test/$PBOPREFIX$"), "test").expect("prefix");
        std::fs::write(root.join("addons/test/config.cpp"), "class CfgPatches {};")
            .expect("config");
        let source = root.join("addons/test/a.sqf");
        std::fs::write(&source, "#include \"header.hpp\"\nhint 'ok';").expect("source");
        let header = root.join("addons/test/header.hpp");
        std::fs::write(&header, "// original").expect("header");
        let request = json!({"workspace":root});
        let mut session = Session::default();
        let first = session.verify(&request).expect("initial");
        assert_eq!(first["checked"], 2);
        assert_eq!(session.verify(&request).expect("repeat")["checked"], 0);
        session
            .cache
            .values_mut()
            .next()
            .expect("cached unit")
            .result["infrastructureError"] = json!("transient test failure");
        let retried = session.verify(&request).expect("retry infrastructure");
        assert_eq!(retried["checked"], 1);
        assert_eq!(retried["units"], first["units"]);
        for (path, text) in [
            (&source, "#include \"header.hpp\"\nprivate _x = ;"),
            (&header, "#define BROKEN 1"),
            (&source, "#include \"missing.hpp\"\nhint 'ok';"),
        ] {
            std::fs::write(path, text).expect("change");
            let incremental = session.verify(&request).expect("incremental");
            let fresh = Session::default().verify(&request).expect("fresh");
            assert_eq!(incremental["units"], fresh["units"]);
            assert!(incremental["checked"].as_u64().expect("count") > 0);
        }
        std::fs::write(root.join("addons/test/missing.hpp"), "// now available")
            .expect("new include");
        let repaired = session.verify(&request).expect("repair");
        assert_eq!(
            repaired["units"],
            Session::default().verify(&request).expect("fresh repair")["units"]
        );
        std::fs::remove_file(&source).expect("delete source");
        assert_eq!(
            session.verify(&request).expect("delete")["units"]
                .as_array()
                .expect("units")
                .len(),
            1
        );
        std::fs::write(root.join(".hemtt/project.toml"), "not valid toml !")
            .expect("invalid rules");
        assert!(session.verify(&request).is_err());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
