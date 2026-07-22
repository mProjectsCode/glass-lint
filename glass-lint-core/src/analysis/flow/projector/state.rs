//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. Joining two
//! reachable environments keeps only equal aliases and common requirement
//! keys, which is the precision boundary that prevents path-local facts from
//! leaking after a control-flow merge.

use std::{collections::BTreeSet, ops::{Deref, DerefMut}};

use crate::{
    analysis::{
        facts::{ControlRegionId, FactId},
        flow::{
            index::FlowId,
            state::{FlowState, FlowStateKey},
        },
        value::{ObjectId, ValueId},
    },
    api::classification::ClassificationEvidence,
};

const MAX_MUTATION_LOG_ENTRIES: usize = 4096;

// ---------------------------------------------------------------------------
// Mutation log for checkpoint/rollback
// ---------------------------------------------------------------------------

/// An inverse delta that can undo one mutation on an alias or state table.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum InverseDelta {
    /// A key/value was inserted (undo: remove by key).
    AliasInsert(ValueId, ObjectId),
    /// A key's value was updated (undo: restore old value).
    AliasUpdate(ValueId, ObjectId, ObjectId),
    /// A key was removed (undo: re-insert with its old value).
    AliasRemove(ValueId, ObjectId),
    /// A state was inserted (undo: remove by key).
    StateInsert(FlowStateKey, FlowState),
    /// A state's requirements changed (undo: restore old state).
    StateUpdate(FlowStateKey, FlowState, FlowState),
    /// A state was removed (undo: re-insert with its old value).
    StateRemove(FlowStateKey, FlowState),
}

/// A position in the persistent mutation history that acts as a checkpoint.
#[allow(dead_code)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Checkpoint(usize);

#[derive(Debug)]
struct LogNode {
    parent: usize,
    depth: usize,
    delta: InverseDelta,
}

/// A bounded parent-linked mutation history. Checkpoints are O(1); moving
/// between them applies only the deltas on the paths between the checkpoints.
#[derive(Debug, Default)]
pub(super) struct MutationLog {
    nodes: Vec<LogNode>,
    cursor: usize,
    budget_exhausted: bool,
}

impl MutationLog {
    fn record(&mut self, delta: InverseDelta) {
        if self.nodes.len() >= MAX_MUTATION_LOG_ENTRIES {
            self.budget_exhausted = true;
            return;
        }
        let parent = self.cursor;
        let depth = self.depth(parent) + 1;
        self.nodes.push(LogNode { parent, depth, delta });
        self.cursor = self.nodes.len();
    }

    /// Record a checkpoint at the current log position.
    #[allow(dead_code)]
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.cursor)
    }

    fn transition(
        &mut self,
        checkpoint: Checkpoint,
        aliases: &mut Vec<(ValueId, ObjectId)>,
        states: &mut Vec<(FlowStateKey, FlowState)>,
    ) -> bool {
        if checkpoint.0 > self.nodes.len() || self.budget_exhausted {
            return false;
        }
        let mut current = self.cursor;
        let mut target = checkpoint.0;
        while self.depth(current) > self.depth(target) { current = self.nodes[current - 1].parent; }
        while self.depth(target) > self.depth(current) { target = self.nodes[target - 1].parent; }
        while current != target {
            current = self.nodes[current - 1].parent;
            target = self.nodes[target - 1].parent;
        }
        let lca = current;
        let mut node = self.cursor;
        while node != lca {
            apply_inverse(&self.nodes[node - 1].delta, aliases, states);
            node = self.nodes[node - 1].parent;
        }
        let mut forward = Vec::new();
        node = checkpoint.0;
        while node != lca { forward.push(node); node = self.nodes[node - 1].parent; }
        for node in forward.into_iter().rev() {
            apply_forward(&self.nodes[node - 1].delta, aliases, states);
        }
        self.cursor = checkpoint.0;
        true
    }

    fn depth(&self, node: usize) -> usize {
        if node == 0 { return 0; }
        self.nodes.get(node.saturating_sub(1)).map_or(0, |entry| entry.depth)
    }

    #[allow(dead_code)]
    pub(super) fn exhausted(&self) -> bool {
        self.budget_exhausted
    }
}

fn apply_inverse(
    delta: &InverseDelta,
    aliases: &mut Vec<(ValueId, ObjectId)>,
    states: &mut Vec<(FlowStateKey, FlowState)>,
) {
    match delta {
        InverseDelta::AliasInsert(value, _) => { let _ = remove_sorted(aliases, value); }
        InverseDelta::AliasUpdate(value, old, _) => {
            if let Ok(pos) = aliases.binary_search_by_key(value, |(key, _)| *key) { aliases[pos].1 = *old; }
        }
        InverseDelta::AliasRemove(value, object) => insert_sorted(aliases, (*value, *object)),
        InverseDelta::StateInsert(key, _) => { let _ = remove_sorted(states, key); }
        InverseDelta::StateUpdate(key, old, _) => {
            if let Ok(pos) = states.binary_search_by_key(key, |(entry, _)| *entry) { states[pos].1 = old.clone(); }
        }
        InverseDelta::StateRemove(key, state) => insert_sorted(states, (*key, state.clone())),
    }
}

fn apply_forward(
    delta: &InverseDelta,
    aliases: &mut Vec<(ValueId, ObjectId)>,
    states: &mut Vec<(FlowStateKey, FlowState)>,
) {
    match delta {
        InverseDelta::AliasInsert(value, object) => insert_sorted(aliases, (*value, *object)),
        InverseDelta::AliasUpdate(value, _, new) => {
            if let Ok(pos) = aliases.binary_search_by_key(value, |(key, _)| *key) { aliases[pos].1 = *new; }
        }
        InverseDelta::AliasRemove(value, _) => { let _ = remove_sorted(aliases, value); }
        InverseDelta::StateInsert(key, state) => insert_sorted(states, (*key, state.clone())),
        InverseDelta::StateUpdate(key, _, new) => {
            if let Ok(pos) = states.binary_search_by_key(key, |(entry, _)| *entry) { states[pos].1 = new.clone(); }
        }
        InverseDelta::StateRemove(key, _) => { let _ = remove_sorted(states, key); }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ReportEvidenceKey {
    rule: usize,
    flow: usize,
    object: ObjectId,
    event: FactId,
}

impl ReportEvidenceKey {
    pub(super) fn new(rule: usize, flow: usize, object: ObjectId, event: FactId) -> Self {
        Self {
            rule,
            flow,
            object,
            event,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// O(1) snapshot of the live tables and reachability at a control boundary.
pub(super) struct FlowEnvironment {
    checkpoint: Checkpoint,
    /// Whether execution can reach the snapshot.
    reachable: bool,
}

#[derive(Debug, Default)]
/// Mutable live alias and object-state tables for one projector pass.
pub(super) struct FlowStateTable {
    /// Current value aliases, keyed by semantic value identity.
    aliases: Vec<(ValueId, ObjectId)>,
    /// Current lifecycle state for each object and flow matcher.
    states: Vec<(FlowStateKey, FlowState)>,
    /// Mutation log for checkpoint/rollback.
    log: MutationLog,
}

impl FlowStateTable {
    pub(super) fn clear(&mut self) {
        let aliases = std::mem::take(&mut self.aliases);
        for (value, object) in aliases { self.log.record(InverseDelta::AliasRemove(value, object)); }
        let states = std::mem::take(&mut self.states);
        for (key, state) in states { self.log.record(InverseDelta::StateRemove(key, state)); }
    }

    pub(super) fn object_for(&self, value: ValueId) -> Option<ObjectId> {
        let pos = self
            .aliases
            .binary_search_by_key(&value, |(k, _)| *k)
            .ok()?;
        Some(self.aliases[pos].1)
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.aliases.iter().map(|(_, object)| *object)
    }

    pub(super) fn bind(&mut self, value: ValueId, object: ObjectId) {
        if let Some(&old) = self
            .aliases
            .binary_search_by_key(&value, |(k, _)| *k)
            .ok()
            .map(|pos| &self.aliases[pos].1)
        {
            self.log
                .record(InverseDelta::AliasUpdate(value, old, object));
            if let Ok(pos) = self.aliases.binary_search_by_key(&value, |(k, _)| *k) {
                self.aliases[pos].1 = object;
            }
        } else {
            self.log.record(InverseDelta::AliasInsert(value, object));
            insert_sorted(&mut self.aliases, (value, object));
        }
    }

    pub(super) fn unbind(&mut self, value: ValueId) -> Option<ObjectId> {
        let pos = self
            .aliases
            .binary_search_by_key(&value, |(k, _)| *k)
            .ok()?;
        let (_, old_object) = self.aliases[pos];
        self.log
            .record(InverseDelta::AliasRemove(value, old_object));
        Some(self.aliases.remove(pos).1)
    }

    pub(super) fn has_alias_for(&self, object: ObjectId) -> bool {
        self.aliases.iter().any(|(_, alias)| *alias == object)
    }

    pub(super) fn states_for(
        &self,
        object: ObjectId,
    ) -> impl Iterator<Item = (FlowStateKey, &FlowState)> {
        self.states
            .iter()
            .filter(move |(key, _)| key.object == object)
            .map(|(key, state)| (*key, state))
    }

    pub(super) fn state(&self, object: ObjectId, flow: FlowId) -> Option<&FlowState> {
        let key = FlowStateKey { object, flow };
        let pos = self.states.binary_search_by_key(&key, |(k, _)| *k).ok()?;
        Some(&self.states[pos].1)
    }

    pub(super) fn state_mut(&mut self, object: ObjectId, flow: FlowId) -> Option<StateEdit<'_>> {
        let key = FlowStateKey { object, flow };
        let pos = self.states.binary_search_by_key(&key, |(k, _)| *k).ok()?;
        let old = self.states[pos].1.clone();
        Some(StateEdit { table: self, key, pos, old })
    }

    pub(super) fn insert_state(&mut self, state: FlowState) {
        let key = state.key();
        match self.states.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(index) => {
                let old = std::mem::replace(&mut self.states[index].1, state.clone());
                self.log.record(InverseDelta::StateUpdate(key, old, state));
            }
            Err(index) => {
                self.states.insert(index, (key, state.clone()));
                self.log.record(InverseDelta::StateInsert(key, state));
            }
        }
    }

    pub(super) fn state_count(&self) -> usize {
        self.states.len()
    }

    pub(super) fn remove_states_for(&mut self, object: ObjectId) {
        let old: Vec<_> = self
            .states
            .iter()
            .filter(|(key, _)| key.object == object)
            .cloned()
            .collect();
        for (key, state) in &old {
            self.log.record(InverseDelta::StateRemove(*key, state.clone()));
        }
        self.states.retain(|(key, _)| key.object != object);
    }

    /// Record a checkpoint at the current mutation log position.
    pub(super) fn capture(&self, reachable: bool) -> FlowEnvironment {
        FlowEnvironment {
            checkpoint: self.log.checkpoint(),
            reachable,
        }
    }

    /// Restore a previously captured environment by rolling back the mutation
    /// log to the checkpoint that corresponds to the environment.
    pub(super) fn restore(&mut self, environment: FlowEnvironment) -> bool {
        if self.log.transition(environment.checkpoint, &mut self.aliases, &mut self.states) {
            environment.reachable
        } else {
            false
        }
    }

    pub(super) fn join_environments(&mut self, environments: &[FlowEnvironment]) -> bool {
        let origin = self.log.checkpoint();
        let mut snapshots = Vec::new();
        for environment in environments.iter().filter(|environment| environment.reachable) {
            if !self.restore(*environment) { return false; }
            snapshots.push((self.aliases.clone(), self.states.clone()));
        }
        if !self.restore(FlowEnvironment { checkpoint: origin, reachable: true }) { return false; }
        let mut snapshot_iter = snapshots.into_iter();
        let Some((mut aliases, mut states)) = snapshot_iter.next() else {
            self.clear();
            return false;
        };
        for (other_aliases, other_states) in snapshot_iter {
            aliases.retain(|(value, object)| {
                other_aliases.binary_search_by_key(value, |(key, _)| *key)
                    .is_ok_and(|pos| other_aliases[pos].1 == *object)
            });
            states.retain_mut(|(key, state)| {
                let Some(pos) = other_states.binary_search_by_key(key, |(entry, _)| *entry).ok() else { return false; };
                state.retain_requirement_keys(&other_states[pos].1);
                true
            });
        }
        self.clear();
        for alias in aliases { self.bind(alias.0, alias.1); }
        for (_, state) in states { self.insert_state(state); }
        true
    }
}

pub(super) struct StateEdit<'a> {
    table: &'a mut FlowStateTable,
    key: FlowStateKey,
    pos: usize,
    old: FlowState,
}

impl Deref for StateEdit<'_> {
    type Target = FlowState;

    fn deref(&self) -> &Self::Target { &self.table.states[self.pos].1 }
}

impl DerefMut for StateEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.table.states[self.pos].1 }
}

impl Drop for StateEdit<'_> {
    fn drop(&mut self) {
        let new = self.table.states[self.pos].1.clone();
        self.table.log.record(InverseDelta::StateUpdate(self.key, self.old.clone(), new));
    }
}

// ---------------------------------------------------------------------------
// Helpers for sorted-vector mutation
// ---------------------------------------------------------------------------

fn insert_sorted<K: Ord, V>(vec: &mut Vec<(K, V)>, entry: (K, V)) {
    match vec.binary_search_by(|(k, _)| k.cmp(&entry.0)) {
        Ok(_) => {}
        Err(index) => vec.insert(index, entry),
    }
}

#[allow(dead_code)]
fn remove_sorted<K: Ord, V>(vec: &mut Vec<(K, V)>, key: &K) -> Option<V> {
    let pos = vec.binary_search_by(|(k, _)| k.cmp(key)).ok()?;
    Some(vec.remove(pos).1)
}

#[derive(Debug)]
/// Per-rule evidence with a bounded deduplication key set.
pub(super) struct FlowEvidence {
    /// Evidence grouped by selected rule index.
    items: Vec<Vec<ClassificationEvidence>>,
    /// `(rule, flow, object, event)` identities already emitted.
    emitted: BTreeSet<ReportEvidenceKey>,
}

impl FlowEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            items: vec![Vec::new(); rule_count],
            emitted: BTreeSet::new(),
        }
    }

    pub(super) fn try_insert(&mut self, key: ReportEvidenceKey, limit: usize) -> bool {
        if !self.emitted.contains(&key) && self.emitted.len() >= limit {
            return false;
        }
        self.emitted.insert(key)
    }

    pub(super) fn record(&mut self, rule_index: usize, evidence: ClassificationEvidence) {
        self.items[rule_index].push(evidence);
    }

    pub(super) fn into_items(self) -> Vec<Vec<ClassificationEvidence>> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_restore_divergent_mutation_paths() {
        let mut table = FlowStateTable::default();
        table.bind(ValueId(1), ObjectId(1));
        let base = table.capture(true);

        table.bind(ValueId(2), ObjectId(2));
        let left = table.capture(true);
        assert!(table.restore(base));
        assert_eq!(table.object_for(ValueId(2)), None);

        table.bind(ValueId(3), ObjectId(3));
        assert!(table.restore(left));
        assert_eq!(table.object_for(ValueId(2)), Some(ObjectId(2)));
        assert_eq!(table.object_for(ValueId(3)), None);
        assert!(table.restore(base));
        assert_eq!(table.object_for(ValueId(1)), Some(ObjectId(1)));
    }
}

#[derive(Debug, Clone)]
/// Saved control construct state used to restore and join environments.
pub(super) enum ControlFrame {
    Branch {
        region: ControlRegionId,
        base: FlowEnvironment,
        then_exit: Option<FlowEnvironment>,
    },
    Loop {
        region: ControlRegionId,
        baseline: FlowEnvironment,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: ControlRegionId,
        baseline: FlowEnvironment,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: ControlRegionId,
        baseline: FlowEnvironment,
        try_exit: Option<FlowEnvironment>,
        catch_exit: Option<FlowEnvironment>,
        normal_exit: Option<FlowEnvironment>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
    },
    Function {
        caller: FlowEnvironment,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Abrupt completion that must be routed through enclosing control frames.
pub(super) enum AbruptExit {
    /// Exit the nearest loop or switch.
    Break,
    /// Continue the nearest loop.
    Continue,
    /// Exit the current function.
    Return,
}

impl FlowEnvironment {
    /// Construct an unreachable environment with no usable state.
    pub(super) fn unreachable() -> Self {
        Self {
            checkpoint: Checkpoint::default(),
            reachable: false,
        }
    }

    /// Whether this snapshot represents a reachable execution path.
    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }
}
