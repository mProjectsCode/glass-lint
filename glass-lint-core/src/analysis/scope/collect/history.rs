//! Source-order assignment state for scope collection.

use std::collections::BTreeMap;

use super::super::BindingProvenance;

#[derive(Debug, Default)]
pub(super) struct AssignmentHistory(BTreeMap<usize, BTreeMap<String, BindingProvenance>>);

impl AssignmentHistory {
    pub(super) fn record(&mut self, scope: usize, name: String, provenance: BindingProvenance) {
        self.0.entry(scope).or_default().insert(name, provenance);
    }

    pub(super) fn get(&self, scope: usize, name: &str) -> Option<&BindingProvenance> {
        self.0
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
    }

    pub(super) fn contains(&self, scope: usize, name: &str) -> bool {
        self.get(scope, name).is_some()
    }
}
