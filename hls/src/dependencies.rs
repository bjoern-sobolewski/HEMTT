//! Reverse include dependencies for incremental compilation units.
use hemtt_workspace::WorkspacePath;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Dependencies {
    roots: HashMap<WorkspacePath, HashSet<WorkspacePath>>,
    consumers: HashMap<WorkspacePath, HashSet<WorkspacePath>>,
}
impl Dependencies {
    pub fn replace(&mut self, root: WorkspacePath, sources: Vec<WorkspacePath>) {
        if let Some(previous) = self.roots.remove(&root) {
            for source in previous {
                if let Some(consumers) = self.consumers.get_mut(&source) {
                    consumers.remove(&root);
                    if consumers.is_empty() {
                        self.consumers.remove(&source);
                    }
                }
            }
        }
        let sources = sources.into_iter().collect::<HashSet<_>>();
        for source in &sources {
            self.consumers
                .entry(source.clone())
                .or_default()
                .insert(root.clone());
        }
        self.roots.insert(root, sources);
    }
    pub fn affected(&self, source: &WorkspacePath, all: bool) -> Vec<WorkspacePath> {
        if all {
            return self
                .roots
                .keys()
                .filter(|root| root.workspace() == source.workspace())
                .cloned()
                .collect();
        }
        let mut roots = self.consumers.get(source).cloned().unwrap_or_default();
        if self.roots.contains_key(source) {
            roots.insert(source.clone());
        }
        roots.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::tests::fixture;
    #[test]
    fn project_changes_do_not_recheck_other_workspaces() {
        let first = fixture("project");
        let second = fixture("plain");
        let root = first.root().join("first.sqf").expect("path");
        let other = second.root().join("other.sqf").expect("path");
        let mut index = Dependencies::default();
        index.replace(root.clone(), Vec::new());
        index.replace(other, Vec::new());
        assert_eq!(index.affected(first.root(), true), vec![root]);
    }

    #[test]
    fn changed_include_edges_are_replaced_and_self_changes_remain_covered() {
        let workspace = fixture("project");
        let root = workspace
            .root()
            .join("addons/valid/script.sqf")
            .expect("path");
        let first = workspace.root().join("first.hpp").expect("path");
        let second = workspace.root().join("second.hpp").expect("path");
        let other = workspace.root().join("other.sqf").expect("path");
        let mut index = Dependencies::default();
        index.replace(root.clone(), vec![first.clone()]);
        index.replace(other.clone(), vec![first.clone()]);
        assert_eq!(index.affected(&first, false).len(), 2);
        index.replace(root.clone(), vec![second.clone()]);
        assert_eq!(index.affected(&first, false), vec![other]);
        assert_eq!(index.affected(&second, false), vec![root.clone()]);
        index.replace(root.clone(), Vec::new());
        assert!(index.affected(&second, false).is_empty());
        assert_eq!(index.affected(&root, false), vec![root]);
        assert_eq!(index.affected(&first, true).len(), 2);
    }
}
