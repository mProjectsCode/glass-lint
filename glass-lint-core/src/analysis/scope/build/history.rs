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
use hashbrown::HashMap;

use crate::analysis::scope::{BindingProvenance, ScopeId, ScopedName};

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
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
            log: Vec::new(),
            cursor: 0,
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
}

/// A checkpointed write set for one control-flow branch.
///
/// Writes are tagged with a generation instead of copied into every
/// checkpoint. Restoring a checkpoint changes only parent-linked deltas,
/// while iteration filters the compact current generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WriteCheckpoint(pub(super) usize);

#[derive(Debug, Clone)]
enum WriteDelta {
    Insert {
        key: ScopedName,
        old: Option<u64>,
        new: u64,
    },
    Generation {
        old: u64,
        new: u64,
    },
}

#[derive(Debug, Clone)]
struct WriteLogEntry {
    parent: usize,
    delta: WriteDelta,
}

#[derive(Debug, Clone)]
pub(super) struct WriteSet {
    entries: HashMap<ScopedName, u64>,
    generation: u64,
    log: Vec<WriteLogEntry>,
    cursor: usize,
}

impl WriteSet {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            log: Vec::new(),
            cursor: 0,
        }
    }

    pub(super) fn insert(&mut self, key: ScopedName) {
        if self.entries.get(&key) == Some(&self.generation) {
            return;
        }
        let old = self.entries.insert(key.clone(), self.generation);
        self.record(WriteDelta::Insert {
            key,
            old,
            new: self.generation,
        });
    }

    pub(super) fn clear(&mut self) {
        let old = self.generation;
        let new = old.saturating_add(1);
        self.generation = new;
        self.record(WriteDelta::Generation { old, new });
    }

    pub(super) fn checkpoint(&self) -> WriteCheckpoint {
        WriteCheckpoint(self.cursor)
    }

    pub(super) fn restore(&mut self, target: WriteCheckpoint) {
        let mut source_path = self.path_to_root(self.cursor);
        let mut target_path = self.path_to_root(target.0);
        let common = source_path
            .iter()
            .zip(&target_path)
            .take_while(|(source, target)| source == target)
            .count();

        for entry in source_path.drain(common..).rev() {
            self.apply_inverse(entry);
        }
        for entry in target_path.drain(common..) {
            self.apply_forward(entry);
        }
        self.cursor = target.0;
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = ScopedName> + '_ {
        self.entries
            .iter()
            .filter(move |(_, generation)| **generation == self.generation)
            .map(|(key, _)| key.clone())
    }

    fn record(&mut self, delta: WriteDelta) {
        let parent = self.cursor;
        self.log.push(WriteLogEntry { parent, delta });
        self.cursor = self.log.len();
    }

    fn path_to_root(&self, mut cursor: usize) -> Vec<usize> {
        let mut path = Vec::new();
        while cursor > 0 {
            path.push(cursor - 1);
            cursor = self.log[cursor - 1].parent;
        }
        path.reverse();
        path
    }

    fn apply_inverse(&mut self, entry: usize) {
        match &self.log[entry].delta {
            WriteDelta::Insert { key, old, .. } => {
                if let Some(old) = old {
                    self.entries.insert(key.clone(), *old);
                } else {
                    self.entries.remove(key);
                }
            }
            WriteDelta::Generation { old, .. } => self.generation = *old,
        }
    }

    fn apply_forward(&mut self, entry: usize) {
        match &self.log[entry].delta {
            WriteDelta::Insert { key, new, .. } => {
                self.entries.insert(key.clone(), *new);
            }
            WriteDelta::Generation { new, .. } => self.generation = *new,
        }
    }
}
#[cfg(test)]
mod tests {
    use glass_lint_datastructures::NameTable;

    use super::*;

    #[test]
    fn assignment_checkpoints_still_restore_values() {
        let mut names = NameTable::default();
        let name = names.intern("value").unwrap();
        let scope = ScopeId::from(1);
        let mut environment = AssignmentEnvironment::new();
        environment.record_known(scope, name, BindingProvenance::Local);
        let base = environment.checkpoint();
        environment.record_unknown(scope, name);
        environment.restore(base);
        assert_eq!(
            environment
                .get_by_id(scope, name)
                .map(|value| &value.provenances[..]),
            Some(&[BindingProvenance::Local][..])
        );
    }

    #[test]
    fn write_set_restores_branch_local_deltas() {
        let mut writes = WriteSet::new();
        let mut names = NameTable::default();
        let first_name = names.intern("first").unwrap();
        let second_name = names.intern("second").unwrap();
        let first = ScopedName::new(ScopeId::from(1), first_name);
        let second = ScopedName::new(ScopeId::from(1), second_name);
        writes.insert(first.clone());
        let base = writes.checkpoint();
        writes.clear();
        writes.insert(second.clone());
        let branch = writes.checkpoint();

        writes.restore(base);
        assert_eq!(writes.iter().collect::<Vec<_>>(), vec![first]);
        writes.restore(branch);
        assert_eq!(writes.iter().collect::<Vec<_>>(), vec![second]);
    }
}
