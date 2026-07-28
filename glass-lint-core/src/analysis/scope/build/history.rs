//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.

use glass_lint_datastructures::NameId;
use hashbrown::{HashMap, HashSet};

use crate::analysis::scope::{BindingProvenance, ScopeId};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProvenanceAlternatives {
    pub(super) provenances: Vec<BindingProvenance>,
    pub(super) unknown: bool,
    pub(super) joined: bool,
    pub(super) exhausted: bool,
}

impl ProvenanceAlternatives {
    pub(super) fn single(provenance: BindingProvenance) -> Self {
        Self {
            provenances: vec![provenance],
            unknown: false,
            joined: false,
            exhausted: false,
        }
    }

    pub(super) fn unknown() -> Self {
        Self {
            provenances: vec![],
            unknown: true,
            joined: false,
            exhausted: false,
        }
    }

    pub(super) fn join_value(mut self) -> Self {
        self.joined = true;
        self
    }

    pub(super) fn add(&mut self, other: &Self) {
        self.unknown |= other.unknown;
        self.exhausted |= other.exhausted;
        self.joined |= other.joined;
        for provenance in &other.provenances {
            if !self.provenances.contains(provenance) {
                self.provenances.push(provenance.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
/// Most recent assignment provenance for each scope-local binding.
pub(super) struct AssignmentEnvironment {
    assignments: HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
        }
    }

    fn ensure_scope(&mut self, scope: ScopeId) -> &mut HashMap<NameId, ProvenanceAlternatives> {
        self.assignments.entry(scope).or_default()
    }

    /// Replace the latest assignment for one scope/name pair with unknown.
    pub(super) fn record_unknown(&mut self, scope: ScopeId, name: NameId) {
        self.ensure_scope(scope)
            .insert(name, ProvenanceAlternatives::unknown());
    }

    pub(super) fn record_known(
        &mut self,
        scope: ScopeId,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.ensure_scope(scope)
            .insert(name, ProvenanceAlternatives::single(provenance));
    }

    pub(super) fn record_alternatives(
        &mut self,
        scope: ScopeId,
        name: NameId,
        alternatives: ProvenanceAlternatives,
    ) {
        self.ensure_scope(scope).insert(name, alternatives);
    }

    pub(super) fn get_by_id(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&ProvenanceAlternatives> {
        self.assignments.get(&scope)?.get(&name)
    }

    pub(super) fn contains_by_id(&self, scope: ScopeId, name: NameId) -> bool {
        self.get_by_id(scope, name).is_some()
    }

    /// Join path environments. Missing entries mean that the incoming value
    /// reaches that path unchanged. Collects distinct provenances from all
    /// paths into alternatives.
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
                let mut value = ProvenanceAlternatives {
                    provenances: Vec::new(),
                    unknown: false,
                    joined: true,
                    exhausted: false,
                };
                for path in paths {
                    if let Some(alt) = path.get_by_id(scope, name) {
                        value.add(alt);
                    }
                }
                result_map.insert(name, value);
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
    fn joins_equal_values_and_collects_distinct_alternatives() {
        let mut names = NameTable::default();
        let name = names.intern("value").unwrap();
        let scope = ScopeId::from(1);
        let mut first = AssignmentEnvironment::new();
        first.record_known(scope, name, BindingProvenance::Local);
        let second = first.clone();
        let joined = AssignmentEnvironment::join(&[&first, &second]);
        assert_eq!(
            joined.get_by_id(scope, name).map(|a| &a.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );

        let mut third = AssignmentEnvironment::new();
        third.record_unknown(scope, name);
        let joined2 = AssignmentEnvironment::join(&[&first, &third]);
        // Local (from first) + empty (from third) = [Local]
        assert_eq!(
            joined2.get_by_id(scope, name).map(|a| &a.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );
    }

    #[test]
    fn join_unions_distinct_provenances() {
        let mut names = NameTable::default();
        let name = names.intern("api").unwrap();
        let scope = ScopeId::from(1);
        let mut path_a = AssignmentEnvironment::new();
        path_a.record_known(
            scope,
            name,
            BindingProvenance::ValueAlias {
                target: glass_lint_datastructures::NamePath::new(),
            },
        );
        let mut path_b = AssignmentEnvironment::new();
        path_b.record_known(scope, name, BindingProvenance::Local);
        let joined = AssignmentEnvironment::join(&[&path_a, &path_b]);
        let alts = joined.get_by_id(scope, name).unwrap();
        assert_eq!(alts.provenances.len(), 2);
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
        assert_eq!(
            joined
                .get_by_id(scope, name)
                .map(|value| &value.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );
    }
}
