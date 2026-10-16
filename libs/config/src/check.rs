//! Checking one preprocessed config file.
//!
//! Shared so the CLI and the language server cannot disagree about what a
//! file's diagnostics are. They have: preprocessor warnings and every note or
//! help were reported by the CLI and never shown in the editor, because each
//! side collected the codes it wanted separately.

use hemtt_common::config::ProjectConfig;
use hemtt_workspace::reporting::{Codes, Processed};

use crate::{ConfigReport, parse};

/// The result of checking one preprocessed config file.
pub struct Checked {
    /// Every diagnostic for the file: preprocessor warnings, then whatever
    /// parsing produced, at every severity.
    pub codes: Codes,
    /// The parsed report, `None` if the file did not parse.
    pub config: Option<ConfigReport>,
}

#[must_use]
/// Check one preprocessed config file.
///
/// Callers are left with what genuinely differs between them - the CLI
/// rapifies and pushes the report to the addon, the language server turns the
/// codes into LSP diagnostics.
pub fn check(processed: &Processed, project: Option<&ProjectConfig>) -> Checked {
    // Preprocessor warnings belong to the file as much as lint codes do
    let mut codes: Codes = processed.warnings().to_vec();
    match parse(project, processed) {
        Ok(report) => {
            // every severity, so a note or help cannot be dropped by one caller
            codes.extend(report.codes().iter().cloned());
            Checked {
                codes,
                config: Some(report),
            }
        }
        Err(errors) => {
            codes.extend(errors);
            Checked {
                codes,
                config: None,
            }
        }
    }
}

/// Check raw-file TOML lints against the same source snapshot as preprocessing.
#[must_use]
pub fn check_with_sources(
    processed: &Processed,
    project: Option<&ProjectConfig>,
    path: &hemtt_workspace::WorkspacePath,
    sources: &hemtt_workspace::SourceDatabase,
) -> Checked {
    let mut checked = check(processed, project);
    if project.is_some_and(|p| !p.toml_lints().is_empty())
        && let Some(report) = &checked.config
    {
        checked.codes.extend(crate::analyze::analyze_toml(
            report.config(),
            sources
                .source_for_path(path)
                .expect("Failed to read config file")
                .1
                .content(),
            path,
            project,
            processed,
        ));
    }
    checked
}

#[cfg(test)]
mod tests {
    use hemtt_workspace::{Workspace, reporting::Codes, reporting::Processed};

    use super::check;

    fn processed(contents: &str) -> Processed {
        use std::io::Write;
        let workspace = Workspace::builder()
            .memory()
            .finish(None, false, &hemtt_common::config::PDriveOption::Disallow)
            .expect("workspace");
        let path = workspace.join("config.cpp").expect("join");
        let mut handle = path.create_file().expect("create");
        handle.write_all(contents.as_bytes()).expect("write");
        drop(handle);
        hemtt_preprocessor::Processor::run(
            &path,
            &hemtt_common::config::PreprocessorOptions::default(),
        )
        .expect("preprocesses")
    }

    fn idents(codes: &Codes) -> Vec<&'static str> {
        codes.iter().map(|code| code.ident()).collect()
    }

    /// The CLI reported these and the editor did not, because each side
    /// collected the codes it wanted separately.
    #[test]
    fn preprocessor_warnings_are_included() {
        let processed = processed("#define A 1\n#define A 2\nclass x {};\n");
        let checked = check(&processed, None);
        let codes = idents(&checked.codes);
        assert!(codes.contains(&"PW1"), "{codes:?}");
        assert!(checked.config.is_some());
    }

    /// The editor took only `warnings()` and `errors()`, so a note or help
    /// never reached it.
    #[test]
    fn every_severity_is_included() {
        let processed = processed("class x {\n    irDotSize = \"0.1/4\";\n};\n");
        let checked = check(&processed, None);
        let codes = idents(&checked.codes);
        assert!(codes.contains(&"L-C12"), "{codes:?}");
    }

    /// The parser recovers where it can, so a malformed file usually still
    /// yields a report - what matters is that the errors come back either way.
    #[test]
    fn a_malformed_file_still_reports() {
        let checked = check(&processed("class x {\n"), None);
        assert!(!checked.codes.is_empty());
        assert!(
            checked
                .codes
                .iter()
                .any(|code| { code.severity() == hemtt_workspace::reporting::Severity::Error })
        );
    }

    #[test]
    fn config_toml_lints_follow_dirty_source_and_discard() {
        use hemtt_common::config::{PDriveOption, PreprocessorOptions, ProjectConfig};
        use hemtt_workspace::SourceDatabase;
        use std::io::Write;
        let project = ProjectConfig::from_file(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/check_project/.hemtt/project.toml"),
        )
        .expect("project");
        let workspace = Workspace::builder()
            .memory()
            .finish(None, false, &PDriveOption::Disallow)
            .expect("workspace");
        let path = workspace.join("config.cpp").expect("path");
        path.create_file()
            .expect("create")
            .write_all(b"class Test {};")
            .expect("write");
        let sources = SourceDatabase::new();
        let id = sources.file_id(&path);
        for (version, text, expected) in [
            (1, "// DIRTY_MARKER\nclass Test {};", true),
            (2, "class Test {};", false),
        ] {
            sources.set_overlay(id, text, version);
            let processed = hemtt_preprocessor::Processor::run_with_sources(
                &path,
                &PreprocessorOptions::default(),
                &sources,
            )
            .expect("preprocess");
            let result = super::check_with_sources(&processed, Some(&project), &path, &sources);
            assert_eq!(idents(&result.codes).contains(&"L-TOML"), expected);
        }
        sources.clear_overlay(id);
        let processed = hemtt_preprocessor::Processor::run_with_sources(
            &path,
            &PreprocessorOptions::default(),
            &sources,
        )
        .expect("preprocess");
        assert!(
            !idents(&super::check_with_sources(&processed, Some(&project), &path, &sources).codes)
                .contains(&"L-TOML")
        );
        assert_eq!(path.read_to_string().expect("disk"), "class Test {};");
    }
}
