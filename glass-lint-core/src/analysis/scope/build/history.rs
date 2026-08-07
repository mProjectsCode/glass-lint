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

use std::sync::atomic::{AtomicU64, Ordering};

use glass_lint_datastructures::{HistoryCursor, HistoryTransition, NameId, ParentLinkedHistory};
use hashbrown::HashMap;

use crate::analysis::scope::{BindingProvenance, ProvenanceAlternatives, ScopeId, ScopedName};

/// Default cap on the number of provenance alternatives per (scope, name)
/// pair. When exceeded, the assignment is marked exhausted and subsequent
/// certainty becomes `Possible` instead of `Definite`.
pub(super) const DEFAULT_ALTERNATIVE_LIMIT: usize = 256;

static NEXT_HISTORY_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryOwner(u64);

impl HistoryOwner {
    fn new() -> Self {
        Self(NEXT_HISTORY_OWNER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HistoryRestoreError {
    ForeignCheckpoint,
}

/// A position in the assignment history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cursor {
    owner: HistoryOwner,
    position: HistoryCursor,
}

#[derive(Debug, Clone)]
struct AssignmentDelta {
    scope: ScopeId,
    name: NameId,
    old: Option<ProvenanceAlternatives>,
    new: ProvenanceAlternatives,
}

#[derive(Debug)]
/// Most recent assignment provenance for each scope-local binding.
///
/// A parent-linked mutation log allows checkpoint-and-restore without cloning
/// the entire assignments map. Restoring to a `Cursor` finds the lowest common
/// ancestor (LCA) of the current and target positions, applies inverse deltas
/// upward from the current position to the LCA, then forward deltas downward
/// from the LCA to the target.
pub(super) struct AssignmentEnvironment {
    assignments: HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    history: ParentLinkedHistory<AssignmentDelta>,
    owner: HistoryOwner,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
            history: ParentLinkedHistory::new(),
            owner: HistoryOwner::new(),
        }
    }

    fn ensure_scope(&mut self, scope: ScopeId) -> &mut HashMap<NameId, ProvenanceAlternatives> {
        self.assignments.entry(scope).or_default()
    }

    /// Record a value change and log it for potential rollback.
    fn record(&mut self, scope: ScopeId, name: NameId, value: ProvenanceAlternatives) {
        let old = self.get_by_id(scope, name).cloned();
        self.ensure_scope(scope).insert(name, value.clone());
        self.history.record(AssignmentDelta {
            scope,
            name,
            old,
            new: value,
        });
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

    /// Record a cursor for later `restore`. O(1).
    pub(super) fn checkpoint(&self) -> Cursor {
        Cursor {
            owner: self.owner,
            position: self.history.checkpoint(),
        }
    }

    /// Restore to a previously recorded cursor by applying forward or inverse
    /// deltas between the current and target positions via LCA.
    /// O(|path|) where path is the number of entries between cursor and target.
    pub(super) fn restore(&mut self, target: Cursor) -> Result<(), HistoryRestoreError> {
        if target.owner != self.owner {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        let assignments = &mut self.assignments;
        if !self
            .history
            .transition(target.position, |direction, delta| match direction {
                HistoryTransition::Undo => apply_assignment_inverse(assignments, delta),
                HistoryTransition::Redo => apply_assignment_forward(assignments, delta),
            })
        {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        Ok(())
    }
}

fn apply_assignment_inverse(
    assignments: &mut HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    delta: &AssignmentDelta,
) {
    if let Some(old) = &delta.old {
        assignments
            .get_mut(&delta.scope)
            .expect("assignment scope must exist while undoing")
            .insert(delta.name, old.clone());
    } else {
        let empty = {
            let scope_map = assignments
                .get_mut(&delta.scope)
                .expect("assignment scope must exist while undoing");
            scope_map.remove(&delta.name);
            scope_map.is_empty()
        };
        if empty {
            assignments.remove(&delta.scope);
        }
    }
}

fn apply_assignment_forward(
    assignments: &mut HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    delta: &AssignmentDelta,
) {
    assignments
        .entry(delta.scope)
        .or_default()
        .insert(delta.name, delta.new.clone());
}

/// A checkpointed write set for one control-flow branch.
///
/// Writes are tagged with a generation instead of copied into every
/// checkpoint. Restoring a checkpoint changes only parent-linked deltas,
/// while iteration filters the compact current generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WriteCheckpoint {
    owner: HistoryOwner,
    position: HistoryCursor,
}

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

#[derive(Debug)]
pub(super) struct WriteSet {
    entries: HashMap<ScopedName, u64>,
    generation: u64,
    history: ParentLinkedHistory<WriteDelta>,
    owner: HistoryOwner,
}

impl WriteSet {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            history: ParentLinkedHistory::new(),
            owner: HistoryOwner::new(),
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
        WriteCheckpoint {
            owner: self.owner,
            position: self.history.checkpoint(),
        }
    }

    pub(super) fn restore(&mut self, target: WriteCheckpoint) -> Result<(), HistoryRestoreError> {
        if target.owner != self.owner {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        let entries = &mut self.entries;
        let generation = &mut self.generation;
        if !self
            .history
            .transition(target.position, |direction, delta| match direction {
                HistoryTransition::Undo => apply_write_inverse(entries, generation, delta),
                HistoryTransition::Redo => apply_write_forward(entries, generation, delta),
            })
        {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        Ok(())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = ScopedName> + '_ {
        self.entries
            .iter()
            .filter(move |(_, generation)| **generation == self.generation)
            .map(|(key, _)| key.clone())
    }

    fn record(&mut self, delta: WriteDelta) {
        self.history.record(delta);
    }
}

fn apply_write_inverse(
    entries: &mut HashMap<ScopedName, u64>,
    generation: &mut u64,
    delta: &WriteDelta,
) {
    match delta {
        WriteDelta::Insert { key, old, .. } => {
            if let Some(old) = old {
                entries.insert(key.clone(), *old);
            } else {
                entries.remove(key);
            }
        }
        WriteDelta::Generation { old, .. } => *generation = *old,
    }
}

fn apply_write_forward(
    entries: &mut HashMap<ScopedName, u64>,
    generation: &mut u64,
    delta: &WriteDelta,
) {
    match delta {
        WriteDelta::Insert { key, new, .. } => {
            entries.insert(key.clone(), *new);
        }
        WriteDelta::Generation { new, .. } => *generation = *new,
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
        let scope = ScopeId::from_test(1);
        let mut environment = AssignmentEnvironment::new();
        environment.record_known(scope, name, BindingProvenance::Local);
        let base = environment.checkpoint();
        environment.record_unknown(scope, name);
        environment
            .restore(base)
            .expect("same assignment history accepts its checkpoint");
        assert_eq!(
            environment
                .get_by_id(scope, name)
                .map(|value| value.complete_witnesses().collect::<Vec<_>>()),
            Some(vec![&BindingProvenance::Local])
        );
    }

    #[test]
    fn assignment_checkpoint_rejects_a_foreign_history() {
        let first = AssignmentEnvironment::new();
        let mut second = AssignmentEnvironment::new();

        assert_eq!(
            second.restore(first.checkpoint()),
            Err(HistoryRestoreError::ForeignCheckpoint)
        );
    }

    #[test]
    fn write_set_restores_branch_local_deltas() {
        let mut writes = WriteSet::new();
        let mut names = NameTable::default();
        let first_name = names.intern("first").unwrap();
        let second_name = names.intern("second").unwrap();
        let first = ScopedName::new(ScopeId::from_test(1), first_name);
        let second = ScopedName::new(ScopeId::from_test(1), second_name);
        writes.insert(first.clone());
        let base = writes.checkpoint();
        writes.clear();
        writes.insert(second.clone());
        let branch = writes.checkpoint();

        writes
            .restore(base)
            .expect("same write history accepts its checkpoint");
        assert_eq!(writes.iter().collect::<Vec<_>>(), vec![first]);
        writes
            .restore(branch)
            .expect("same write history accepts its checkpoint");
        assert_eq!(writes.iter().collect::<Vec<_>>(), vec![second]);
    }

    #[test]
    fn write_checkpoint_rejects_a_foreign_history() {
        let first = WriteSet::new();
        let mut second = WriteSet::new();

        assert_eq!(
            second.restore(first.checkpoint()),
            Err(HistoryRestoreError::ForeignCheckpoint)
        );
    }
}
