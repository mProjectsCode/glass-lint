//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use std::collections::BTreeMap;

use crate::analysis::{
    name::{NameId, NameTable},
    scope::{BindingProvenance, ScopeId},
};

#[derive(Debug)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentHistory {
    assignments: BTreeMap<ScopeId, BTreeMap<NameId, BindingProvenance>>,
}

impl AssignmentHistory {
    pub(super) fn new() -> Self {
        Self {
            assignments: BTreeMap::new(),
        }
    }

    /// Replace the latest assignment for one scope/name pair.
    pub(super) fn record(
        &mut self,
        names: &NameTable,
        scope: ScopeId,
        name: &str,
        provenance: BindingProvenance,
    ) {
        let Some(name) = names.lookup(name) else {
            return;
        };
        self.assignments
            .entry(scope)
            .or_default()
            .insert(name, provenance);
    }

    /// Return the latest assignment visible in one lexical scope.
    pub(super) fn get(
        &self,
        names: &NameTable,
        scope: ScopeId,
        name: &str,
    ) -> Option<&BindingProvenance> {
        let name = names.lookup(name)?;
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(&name))
    }

    /// Whether an assignment has been recorded for the scope/name pair.
    pub(super) fn contains(&self, names: &NameTable, scope: ScopeId, name: &str) -> bool {
        self.get(names, scope, name).is_some()
    }
}
