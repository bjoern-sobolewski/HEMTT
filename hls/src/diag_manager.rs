use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, LazyLock, RwLock},
};
use tower_lsp::{Client, lsp_types::Diagnostic};
use url::Url;

#[derive(Clone)]
pub struct DiagManager {
    worker: Arc<DiagWorker>,
}
static SINGLETON: LazyLock<RwLock<Option<DiagManager>>> = LazyLock::new(|| RwLock::new(None));

impl DiagManager {
    pub fn get() -> Option<Self> {
        SINGLETON.read().expect("diagnostics lock").clone()
    }
    pub fn init(client: Client) {
        SINGLETON
            .write()
            .expect("diagnostics lock")
            .get_or_insert_with(|| Self {
                worker: Arc::new(DiagWorker {
                    client,
                    state: RwLock::new(DiagnosticState::default()),
                    published: tokio::sync::Mutex::new(HashMap::new()),
                }),
            });
    }
    pub fn replace_scope(&self, scope: String, findings: HashMap<Url, Vec<Diagnostic>>) {
        self.worker
            .state
            .write()
            .expect("diagnostics lock")
            .replace(scope, findings);
    }
    pub async fn sync(&self, _scope: &str) {
        self.worker.sync().await;
    }
}

/// Indexed by scope and file so a single-file edit never scans or republishes
/// the entire workspace. Replacing a compilation unit is atomic: findings from
/// other SQF/config consumers remain present, including shared-header findings.
#[derive(Default)]
struct DiagnosticState {
    scopes: HashMap<String, HashSet<Url>>,
    files: HashMap<Url, BTreeMap<String, Vec<Diagnostic>>>,
    dirty: HashSet<Url>,
}
impl DiagnosticState {
    fn replace(&mut self, scope: String, findings: HashMap<Url, Vec<Diagnostic>>) {
        if let Some(previous) = self.scopes.remove(&scope) {
            for file in previous {
                if let Some(scopes) = self.files.get_mut(&file) {
                    scopes.remove(&scope);
                    if scopes.is_empty() {
                        self.files.remove(&file);
                    }
                }
                self.dirty.insert(file);
            }
        }
        let mut touched = HashSet::new();
        for (file, diagnostics) in findings {
            if diagnostics.is_empty() {
                continue;
            }
            self.files
                .entry(file.clone())
                .or_default()
                .insert(scope.clone(), diagnostics);
            touched.insert(file.clone());
            self.dirty.insert(file);
        }
        if !touched.is_empty() {
            self.scopes.insert(scope, touched);
        }
    }

    fn take_changes(
        &mut self,
        published: &mut HashMap<Url, Vec<Diagnostic>>,
    ) -> Vec<(Url, Vec<Diagnostic>)> {
        let mut updates = Vec::new();
        for file in std::mem::take(&mut self.dirty) {
            // Stable scope order prevents unchanged merged diagnostics from
            // looking different just because a HashMap changed iteration order.
            let current = self
                .files
                .get(&file)
                .map(|scopes| scopes.values().flatten().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            if published.get(&file).map(Vec::as_slice).unwrap_or_default() == current.as_slice() {
                continue;
            }
            if current.is_empty() {
                published.remove(&file);
            } else {
                published.insert(file.clone(), current.clone());
            }
            updates.push((file, current));
        }
        updates
    }
}

struct DiagWorker {
    client: Client,
    state: RwLock<DiagnosticState>,
    published: tokio::sync::Mutex<HashMap<Url, Vec<Diagnostic>>>,
}
impl DiagWorker {
    async fn sync(&self) {
        let _timer = crate::performance::Timer::start("diagnostics.sync");
        let mut published = self.published.lock().await;
        let updates = self
            .state
            .write()
            .expect("diagnostics lock")
            .take_changes(&mut published);
        for (file, diagnostics) in updates {
            self.client
                .publish_diagnostics(file, diagnostics, None)
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn diagnostic(message: &str) -> Diagnostic {
        Diagnostic {
            message: message.into(),
            ..Default::default()
        }
    }
    #[test]
    fn only_changed_files_are_published_and_repair_clears_once() {
        let mut state = DiagnosticState::default();
        let mut published = HashMap::new();
        let file = Url::parse("file:///one.sqf").expect("url");
        let unchanged = Url::parse("file:///two.sqf").expect("url");
        state.replace(
            "one".into(),
            HashMap::from([(file.clone(), vec![diagnostic("error")])]),
        );
        state.replace(
            "two".into(),
            HashMap::from([(unchanged.clone(), vec![diagnostic("keep")])]),
        );
        assert_eq!(state.take_changes(&mut published).len(), 2);
        state.replace(
            "one".into(),
            HashMap::from([(file.clone(), vec![diagnostic("error")])]),
        );
        assert!(state.take_changes(&mut published).is_empty());
        state.replace("one".into(), HashMap::new());
        assert_eq!(state.take_changes(&mut published), vec![(file, Vec::new())]);
        assert!(state.take_changes(&mut published).is_empty());
        assert_eq!(published[&unchanged][0].message, "keep");
    }
    #[test]
    fn config_and_sqf_consumers_preserve_each_others_findings() {
        let mut state = DiagnosticState::default();
        let mut published = HashMap::new();
        let file = Url::parse("file:///shared.hpp").expect("url");
        state.replace(
            "config".into(),
            HashMap::from([(file.clone(), vec![diagnostic("config")])]),
        );
        state.replace(
            "sqf".into(),
            HashMap::from([(file.clone(), vec![diagnostic("sqf")])]),
        );
        assert_eq!(state.take_changes(&mut published)[0].1.len(), 2);
        state.replace("sqf".into(), HashMap::new());
        assert_eq!(
            state.take_changes(&mut published),
            vec![(file.clone(), vec![diagnostic("config")])]
        );
        state.replace("config".into(), HashMap::new());
        assert_eq!(state.take_changes(&mut published), vec![(file, Vec::new())]);
    }
}
