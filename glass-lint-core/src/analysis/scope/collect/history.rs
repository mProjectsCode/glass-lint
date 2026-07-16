//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use std::collections::BTreeMap;

use super::super::{BindingProvenance, ScopeId};

#[derive(Debug, Default)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentHistory(BTreeMap<ScopeId, BTreeMap<String, BindingProvenance>>);

impl AssignmentHistory {
    /// Replace the latest assignment for one scope/name pair.
    pub(super) fn record(&mut self, scope: ScopeId, name: String, provenance: BindingProvenance) {
        self.0.entry(scope).or_default().insert(name, provenance);
    }

    /// Return the latest assignment visible in one lexical scope.
    pub(super) fn get(&self, scope: ScopeId, name: &str) -> Option<&BindingProvenance> {
        self.0
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
    }

    /// Whether an assignment has been recorded for the scope/name pair.
    pub(super) fn contains(&self, scope: ScopeId, name: &str) -> bool {
        self.get(scope, name).is_some()
    }
}
