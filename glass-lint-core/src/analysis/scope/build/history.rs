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

use crate::analysis::scope::{ProvenanceAlternatives, ScopeId, ScopedName};

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
    /// The checkpoint belongs to another history owner or is outside this
    /// history's reachable mutation-log positions.
    ForeignCheckpoint,
    /// The delta log and the live state disagreed while applying a delta.
    StateDesync,
}

/// A position in the assignment history.
///
/// Checkpoints are created only by the single history owner held by the
/// collector's path state; the owner marker rejects cross-history restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cursor {
    position: HistoryCheckpoint,
}

/// An owner-qualified position in one mutation history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryCheckpoint {
    owner: HistoryOwner,
    position: HistoryCursor,
}

#[derive(Debug)]
/// A mutation log with checkpoints valid only for this history instance.
struct OwnedHistory<D> {
    history: ParentLinkedHistory<D>,
    owner: HistoryOwner,
}

impl<D> OwnedHistory<D> {
    fn new() -> Self {
        Self {
            history: ParentLinkedHistory::new(),
            owner: HistoryOwner::new(),
        }
    }

    fn checkpoint(&self) -> HistoryCheckpoint {
        HistoryCheckpoint {
            owner: self.owner,
            position: self.history.checkpoint(),
        }
    }

    fn record(&mut self, delta: D) {
        self.history.record(delta);
    }

    fn transition(
        &mut self,
        target: HistoryCheckpoint,
        mut apply: impl FnMut(HistoryTransition, &D) -> bool,
    ) -> Result<(), HistoryRestoreError> {
        if target.owner != self.owner {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        let mut desync = false;
        let reachable = self
            .history
            .transition(target.position, |direction, delta| {
                if !apply(direction, delta) {
                    desync = true;
                }
            });
        if !reachable {
            return Err(HistoryRestoreError::ForeignCheckpoint);
        }
        if desync {
            return Err(HistoryRestoreError::StateDesync);
        }
        Ok(())
    }
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
    history: OwnedHistory<AssignmentDelta>,
}

impl AssignmentEnvironment {
    pub(super) fn new() -> Self {
        Self {
            assignments: HashMap::new(),
            history: OwnedHistory::new(),
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
            position: self.history.checkpoint(),
        }
    }

    /// Restore to a previously recorded cursor by applying forward or inverse
    /// deltas between the current and target positions via LCA.
    /// O(|path|) where path is the number of entries between cursor and target.
    pub(super) fn restore(&mut self, target: Cursor) -> Result<(), HistoryRestoreError> {
        let assignments = &mut self.assignments;
        self.history
            .transition(target.position, |direction, delta| match direction {
                HistoryTransition::Undo => apply_assignment_inverse(assignments, delta),
                HistoryTransition::Redo => apply_assignment_forward(assignments, delta),
            })
    }
}

fn apply_assignment_inverse(
    assignments: &mut HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    delta: &AssignmentDelta,
) -> bool {
    let Some(scope_map) = assignments.get_mut(&delta.scope) else {
        return false;
    };
    if let Some(old) = &delta.old {
        scope_map.insert(delta.name, old.clone());
        return true;
    }
    let empty = {
        scope_map.remove(&delta.name);
        scope_map.is_empty()
    };
    if empty {
        assignments.remove(&delta.scope);
    }
    true
}

fn apply_assignment_forward(
    assignments: &mut HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>,
    delta: &AssignmentDelta,
) -> bool {
    assignments
        .entry(delta.scope)
        .or_default()
        .insert(delta.name, delta.new.clone());
    true
}

/// A checkpointed write set for one control-flow branch.
///
/// Writes are tagged with a generation instead of copied into every
/// checkpoint. Restoring a checkpoint changes only parent-linked deltas,
/// while iteration filters the compact current generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WriteCheckpoint {
    position: HistoryCheckpoint,
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
    history: OwnedHistory<WriteDelta>,
}

impl WriteSet {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            history: OwnedHistory::new(),
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
            position: self.history.checkpoint(),
        }
    }

    pub(super) fn restore(&mut self, target: WriteCheckpoint) -> Result<(), HistoryRestoreError> {
        let entries = &mut self.entries;
        let generation = &mut self.generation;
        self.history
            .transition(target.position, |direction, delta| match direction {
                HistoryTransition::Undo => apply_write_inverse(entries, generation, delta),
                HistoryTransition::Redo => apply_write_forward(entries, generation, delta),
            })
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
) -> bool {
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
    true
}

fn apply_write_forward(
    entries: &mut HashMap<ScopedName, u64>,
    generation: &mut u64,
    delta: &WriteDelta,
) -> bool {
    match delta {
        WriteDelta::Insert { key, new, .. } => {
            entries.insert(key.clone(), *new);
        }
        WriteDelta::Generation { new, .. } => *generation = *new,
    }
    true
}
#[cfg(test)]
mod tests;
