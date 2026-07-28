//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use glass_lint_datastructures::NameId;
use hashbrown::{HashMap, HashSet};

use crate::analysis::scope::{BindingProvenance, ScopeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AssignmentValue {
    Known(BindingProvenance),
    Unknown,
}

#[derive(Debug, Clone)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentEnvironment {
    assignments: HashMap<ScopeId, HashMap<NameId, AssignmentValue>>,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
        }
    }

    fn ensure_scope(&mut self, scope: ScopeId) -> &mut HashMap<NameId, AssignmentValue> {
        self.assignments.entry(scope).or_default()
    }

    /// Replace the latest assignment for one scope/name pair.
    pub(super) fn record_unknown(&mut self, scope: ScopeId, name: NameId) {
        self.ensure_scope(scope)
            .insert(name, AssignmentValue::Unknown);
    }

    pub(super) fn record_known(
        &mut self,
        scope: ScopeId,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.ensure_scope(scope)
            .insert(name, AssignmentValue::Known(provenance));
    }

    pub(super) fn get_by_id(&self, scope: ScopeId, name: NameId) -> Option<&AssignmentValue> {
        self.assignments.get(&scope)?.get(&name)
    }

    pub(super) fn contains_by_id(&self, scope: ScopeId, name: NameId) -> bool {
        self.get_by_id(scope, name).is_some()
    }

    /// Join path environments. Missing entries mean that the incoming value
    /// reaches that path unchanged; disagreement is retained as unknown.
    pub(super) fn join(paths: &[&Self]) -> Self {
        let mut active_scopes = paths
            .iter()
            .flat_map(|path| path.assignments.keys().copied())
            .collect::<Vec<_>>();
        active_scopes.sort_unstable();
        active_scopes.dedup();

        let mut joined = HashMap::new();

        for scope in active_scopes {
            let mut result_map = HashMap::new();
            let mut all_names = HashSet::new();
            for path in paths {
                if let Some(map) = path.assignments.get(&scope) {
                    all_names.extend(map.keys().copied());
                }
            }
            for name in all_names {
                let first = paths[0].get_by_id(scope, name);
                if paths
                    .iter()
                    .all(|path| path.get_by_id(scope, name) == first)
                {
                    if let Some(value) = first {
                        result_map.insert(name, value.clone());
                    }
                } else {
                    result_map.insert(name, AssignmentValue::Unknown);
                }
            }
            if !result_map.is_empty() {
                joined.insert(scope, result_map);
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

    #[test]
    fn sparse_scope_join_does_not_materialize_empty_scopes() {
        let mut names = NameTable::default();
        let name = names.intern("value").unwrap();
        let scope = ScopeId::from(100_000);
        let mut first = AssignmentEnvironment::new();
        first.record_known(scope, name, BindingProvenance::Local);

        let joined = AssignmentEnvironment::join(&[&first]);

        assert_eq!(joined.assignments.len(), 1);
        assert_eq!(joined.get_by_id(scope, name), first.get_by_id(scope, name));
    }
}
