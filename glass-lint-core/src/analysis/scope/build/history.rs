//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{NameId, NameTable};

use crate::analysis::scope::{BindingProvenance, ScopeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AssignmentValue {
    Known(BindingProvenance),
    Unknown,
}

#[derive(Debug, Clone)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentEnvironment {
    assignments: BTreeMap<ScopeId, BTreeMap<NameId, AssignmentValue>>,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: BTreeMap::new(),
        }
    }

    /// Replace the latest assignment for one scope/name pair.
    pub(super) fn record_unknown(&mut self, scope: ScopeId, name: NameId) {
        self.assignments
            .entry(scope)
            .or_default()
            .insert(name, AssignmentValue::Unknown);
    }

    pub(super) fn record_known(
        &mut self,
        scope: ScopeId,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.assignments
            .entry(scope)
            .or_default()
            .insert(name, AssignmentValue::Known(provenance));
    }

    /// Return the latest assignment visible in one lexical scope.
    pub(super) fn get(
        &self,
        names: &NameTable,
        scope: ScopeId,
        name: &str,
    ) -> Option<&AssignmentValue> {
        let name = names.lookup(name)?;
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(&name))
    }

    /// Whether an assignment has been recorded for the scope/name pair.
    pub(super) fn contains(&self, names: &NameTable, scope: ScopeId, name: &str) -> bool {
        self.get(names, scope, name).is_some()
    }

    pub(super) fn get_by_id(&self, scope: ScopeId, name: NameId) -> Option<&AssignmentValue> {
        self.assignments.get(&scope)?.get(&name)
    }

    /// Join path environments. Missing entries mean that the incoming value
    /// reaches that path unchanged; disagreement is retained as unknown.
    pub(super) fn join(paths: &[&Self]) -> Self {
        let mut keys = BTreeMap::<ScopeId, BTreeSet<NameId>>::new();
        for path in paths {
            for (scope, assignments) in &path.assignments {
                let names = keys.entry(*scope).or_default();
                for name in assignments.keys() {
                    names.insert(*name);
                }
            }
        }

        let mut joined = BTreeMap::new();
        for (scope, names) in keys {
            let mut assignments = BTreeMap::new();
            for name in names {
                let first = paths[0].get_by_id(scope, name);
                if paths
                    .iter()
                    .all(|path| path.get_by_id(scope, name) == first)
                {
                    if let Some(value) = first {
                        assignments.insert(name, value.clone());
                    }
                } else {
                    assignments.insert(name, AssignmentValue::Unknown);
                }
            }
            if !assignments.is_empty() {
                joined.insert(scope, assignments);
            }
        }
        Self {
            assignments: joined,
        }
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::NameTable;

    use super::*;

    #[test]
    fn joins_equal_values_and_marks_disagreement_unknown() {
        let mut names = NameTable::default();
        let name = names.intern("value").unwrap();
        let scope = ScopeId::from(1);
        let mut first = AssignmentEnvironment::new();
        first.record_known(scope, name, BindingProvenance::Local);
        let second = first.clone();
        assert_eq!(
            AssignmentEnvironment::join(&[&first, &second]).get_by_id(scope, name),
            Some(&AssignmentValue::Known(BindingProvenance::Local))
        );

        let mut third = AssignmentEnvironment::new();
        third.record_unknown(scope, name);
        assert_eq!(
            AssignmentEnvironment::join(&[&first, &third]).get_by_id(scope, name),
            Some(&AssignmentValue::Unknown)
        );
    }
}
