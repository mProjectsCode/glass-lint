//! Source-order assignment state for scope collection.
//!
//! The latest value is indexed by lexical scope and name. This lets
//! use-position queries distinguish a declaration's initial provenance from a
//! later reassignment without mutating the declaration map.
//!
//! A parent-linked mutation log avoids deep-cloning the entire environment on
//! each checkpoint. `restore` transitions between log positions by finding the
//! lowest common ancestor (LCA) and applying only the diff. This is the same
//! approach as the flow projector's `MutationLog`.

use glass_lint_datastructures::NameId;
use hashbrown::{HashMap, HashSet};

use crate::analysis::scope::{BindingProvenance, ScopeId};

/// Default cap on the number of provenance alternatives per (scope, name)
/// pair. When exceeded, the assignment is marked exhausted and subsequent
/// certainty becomes `Possible` instead of `Definite`.
pub(super) const DEFAULT_ALTERNATIVE_LIMIT: usize = 256;

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

    /// Same as `add`, but bounded by `limit` — when the limit is exceeded
    /// the result is marked exhausted.
    pub(super) fn add_bounded(&mut self, other: &Self, limit: usize) {
        self.unknown |= other.unknown;
        self.exhausted |= other.exhausted;
        self.joined |= other.joined;
        for provenance in &other.provenances {
            if self.provenances.len() >= limit {
                self.exhausted = true;
                self.unknown = true;
                return;
            }
            if !self.provenances.contains(provenance) {
                self.provenances.push(provenance.clone());
            }
        }
    }
}

/// A position in the parent-linked mutation log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cursor(pub(super) usize);

/// A single mutation recorded in the log, parent-linked to the previous
/// cursor position so arbitrary transitions are always valid.
#[derive(Debug, Clone)]
struct LogEntry {
    /// Cursor position at the time this entry was recorded.
    parent: usize,
    scope: ScopeId,
    name: NameId,
    old: Option<ProvenanceAlternatives>,
    new: ProvenanceAlternatives,
}

#[derive(Debug, Clone)]
/// Most recent assignment provenance for each scope-local binding.
///
/// A parent-linked mutation log allows checkpoint-and-restore without cloning
/// the entire assignments map. Restoring to a `Cursor` finds the lowest common
/// ancestor (LCA) of the current and target positions, applies inverse deltas
/// upward from the current position to the LCA, then forward deltas downward
/// from the LCA to the target.
pub(super) struct AssignmentEnvironment {
    assignments: HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    log: Vec<LogEntry>,
    /// Current position in the mutation tree. 0 = root (no entries applied).
    cursor: usize,
    /// Maximum number of provenance alternatives per binding before
    /// exhaustion is signalled.
    alternative_limit: usize,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
            log: Vec::new(),
            cursor: 0,
            alternative_limit: DEFAULT_ALTERNATIVE_LIMIT,
        }
    }

    fn ensure_scope(&mut self, scope: ScopeId) -> &mut HashMap<NameId, ProvenanceAlternatives> {
        self.assignments.entry(scope).or_default()
    }

    /// Walk the parent chain from `cursor` to the root (0), returning entry
    /// indices in forward order (root → leaf).
    fn path_to_root(&self, cursor: usize) -> Vec<usize> {
        let mut stack = Vec::new();
        let mut c = cursor;
        while c > 0 {
            stack.push(c - 1);
            c = self.log[c - 1].parent;
        }
        let mut path = Vec::with_capacity(stack.len());
        while let Some(idx) = stack.pop() {
            path.push(idx);
        }
        path
    }

    fn apply_forward(&mut self, entry_idx: usize) {
        let scope = self.log[entry_idx].scope;
        let name = self.log[entry_idx].name;
        let new = self.log[entry_idx].new.clone();
        self.ensure_scope(scope).insert(name, new);
    }

    fn apply_inverse(&mut self, entry_idx: usize) {
        let scope = self.log[entry_idx].scope;
        let name = self.log[entry_idx].name;
        if let Some(ref old) = self.log[entry_idx].old {
            self.assignments
                .get_mut(&scope)
                .unwrap()
                .insert(name, old.clone());
        } else {
            let empty = {
                let scope_map = self.assignments.get_mut(&scope).unwrap();
                scope_map.remove(&name);
                scope_map.is_empty()
            };
            if empty {
                self.assignments.remove(&scope);
            }
        }
    }

    /// Record a value change and log it for potential rollback.
    fn record(&mut self, scope: ScopeId, name: NameId, value: ProvenanceAlternatives) {
        let old = self.get_by_id(scope, name).cloned();
        self.ensure_scope(scope).insert(name, value.clone());
        self.log.push(LogEntry {
            parent: self.cursor,
            scope,
            name,
            old,
            new: value,
        });
        self.cursor = self.log.len();
    }

    /// Replace the latest assignment for one scope/name pair with unknown.
    pub(super) fn record_unknown(&mut self, scope: ScopeId, name: NameId) {
        self.record(scope, name, ProvenanceAlternatives::unknown());
    }

    pub(super) fn record_known(
        &mut self,
        scope: ScopeId,
        name: NameId,
        provenance: BindingProvenance,
    ) {
        self.record(scope, name, ProvenanceAlternatives::single(provenance));
    }

    pub(super) fn record_alternatives(
        &mut self,
        scope: ScopeId,
        name: NameId,
        alternatives: ProvenanceAlternatives,
    ) {
        self.record(scope, name, alternatives);
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

    /// Record a cursor for later `restore`. O(1).
    pub(super) fn checkpoint(&self) -> Cursor {
        Cursor(self.cursor)
    }

    /// Restore to a previously recorded cursor by applying forward or inverse
    /// deltas between the current and target positions via LCA.
    /// O(|path|) where path is the number of entries between cursor and target.
    pub(super) fn restore(&mut self, target: Cursor) {
        let src_path = self.path_to_root(self.cursor);
        let dst_path = self.path_to_root(target.0);

        let common = src_path
            .iter()
            .zip(dst_path.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // Undo from current position to LCA
        for &entry_idx in src_path[common..].iter().rev() {
            self.apply_inverse(entry_idx);
        }

        // Redo from LCA to target
        for &entry_idx in &dst_path[common..] {
            self.apply_forward(entry_idx);
        }

        self.cursor = target.0;
    }

    /// Full clone of the environment for joining. Call only at join points.
    pub(super) fn snapshot(&self) -> Self {
        Self {
            assignments: self.assignments.clone(),
            log: Vec::new(),
            cursor: 0,
            alternative_limit: self.alternative_limit,
        }
    }

    /// Join path environments. Missing entries mean that the incoming value
    /// reaches that path unchanged. Collects distinct provenances from all
    /// paths into alternatives, bounded by `alternative_limit`.
    pub(super) fn join(paths: &[&Self], alternative_limit: usize) -> Self {
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
                        value.add_bounded(alt, alternative_limit);
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
            log: Vec::new(),
            cursor: 0,
            alternative_limit,
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
        let joined = AssignmentEnvironment::join(&[&first, &second], DEFAULT_ALTERNATIVE_LIMIT);
        assert_eq!(
            joined.get_by_id(scope, name).map(|a| &a.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );

        let mut third = AssignmentEnvironment::new();
        third.record_unknown(scope, name);
        let joined2 = AssignmentEnvironment::join(&[&first, &third], DEFAULT_ALTERNATIVE_LIMIT);
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
        let joined = AssignmentEnvironment::join(&[&path_a, &path_b], DEFAULT_ALTERNATIVE_LIMIT);
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

        let joined = AssignmentEnvironment::join(&[&first], DEFAULT_ALTERNATIVE_LIMIT);

        assert_eq!(joined.assignments.len(), 1);
        assert_eq!(
            joined
                .get_by_id(scope, name)
                .map(|value| &value.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );
    }
}
