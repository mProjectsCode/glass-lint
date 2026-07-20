//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use std::collections::BTreeMap;

use crate::analysis::{
    name::{NameId, NameTableCtx},
    scope::{BindingProvenance, ScopeId},
};

#[derive(Debug)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentHistory<'a> {
    names: NameTableCtx<'a>,
    assignments: BTreeMap<ScopeId, BTreeMap<NameId, BindingProvenance>>,
}

impl<'a> AssignmentHistory<'a> {
    pub(super) fn new(names: NameTableCtx<'a>) -> Self {
        Self {
            names,
            assignments: BTreeMap::new(),
        }
    }

    /// Replace the latest assignment for one scope/name pair.
    pub(super) fn record(&mut self, scope: ScopeId, name: &str, provenance: BindingProvenance) {
        let Ok(name) = self.names.intern(name) else {
            return;
        };
        self.assignments
            .entry(scope)
            .or_default()
            .insert(name, provenance);
    }

    /// Return the latest assignment visible in one lexical scope.
    pub(super) fn get(&self, scope: ScopeId, name: &str) -> Option<&BindingProvenance> {
        let name = self.names.lookup(name)?;
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(&name))
    }

    /// Whether an assignment has been recorded for the scope/name pair.
    pub(super) fn contains(&self, scope: ScopeId, name: &str) -> bool {
        self.get(scope, name).is_some()
    }
}
