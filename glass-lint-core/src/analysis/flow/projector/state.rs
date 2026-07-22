//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. Joining two
//! reachable environments keeps only equal aliases and common requirement
//! keys, which is the precision boundary that prevents path-local facts from
//! leaking after a control-flow merge.

use std::collections::BTreeSet;

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
    StateUpdate(FlowStateKey, FlowState),
    /// A state was removed (undo: re-insert with its old value).
    StateRemove(FlowStateKey, FlowState),
}

/// A position in the mutation log that acts as a rollback checkpoint.
#[allow(dead_code)]
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Checkpoint(usize);

/// A bounded mutation log that enables O(1) branch capture and O(delta)
/// rollback without cloning complete alias or state tables.
#[derive(Debug, Default)]
pub(super) struct MutationLog {
    entries: Vec<InverseDelta>,
    budget_exhausted: bool,
}

impl MutationLog {
    fn record(&mut self, delta: InverseDelta) {
        if self.entries.len() >= MAX_MUTATION_LOG_ENTRIES {
            self.budget_exhausted = true;
            return;
        }
        self.entries.push(delta);
    }

    /// Record a checkpoint at the current log position.
    #[allow(dead_code)]
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.entries.len())
    }

    /// Roll back all mutations recorded since `checkpoint`, applying inverse
    /// deltas to the given tables.
    #[allow(dead_code)]
    fn rollback(
        &mut self,
        checkpoint: Checkpoint,
        aliases: &mut Vec<(ValueId, ObjectId)>,
        states: &mut Vec<(FlowStateKey, FlowState)>,
    ) {
        while self.entries.len() > checkpoint.0 {
            match self.entries.pop().unwrap() {
                InverseDelta::AliasInsert(value, _object) => {
                    let _ = remove_sorted(aliases, &value);
                }
                InverseDelta::AliasUpdate(value, old, _new) => {
                    if let Ok(pos) = aliases.binary_search_by_key(&value, |(k, _)| *k) {
                        aliases[pos].1 = old;
                    }
                }
                InverseDelta::AliasRemove(value, old_object) => {
                    insert_sorted(aliases, (value, old_object));
                }
                InverseDelta::StateInsert(key, _state) => {
                    let _ = remove_sorted(states, &key);
                }
                InverseDelta::StateUpdate(key, old_state) => {
                    if let Ok(pos) = states.binary_search_by_key(&key, |(k, _)| *k) {
                        states[pos].1 = old_state;
                    }
                }
                InverseDelta::StateRemove(key, old_state) => {
                    insert_sorted(states, (key, old_state));
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn exhausted(&self) -> bool {
        self.budget_exhausted
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Snapshot of aliases, flow states, and reachability at a control boundary.
pub(super) struct FlowEnvironment {
    /// Value-to-object aliases proven on the snapshot path.
    aliases: Vec<(ValueId, ObjectId)>,
    /// Object/flow lifecycle states proven on the snapshot path.
    states: Vec<(FlowStateKey, FlowState)>,
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
        self.aliases.clear();
        self.states.clear();
        self.log = MutationLog::default();
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

    pub(super) fn state_mut(&mut self, object: ObjectId, flow: FlowId) -> Option<&mut FlowState> {
        let key = FlowStateKey { object, flow };
        let pos = self.states.binary_search_by_key(&key, |(k, _)| *k).ok()?;
        let old = self.states[pos].1.clone();
        self.log.record(InverseDelta::StateUpdate(key, old));
        Some(&mut self.states[pos].1)
    }

    pub(super) fn insert_state(&mut self, state: FlowState) {
        let key = state.key();
        self.log
            .record(InverseDelta::StateInsert(key, state.clone()));
        match self.states.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(index) => self.states[index].1 = state,
            Err(index) => self.states.insert(index, (key, state)),
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
            self.log
                .record(InverseDelta::StateRemove(*key, state.clone()));
        }
        self.states.retain(|(key, _)| key.object != object);
    }

    /// Record a checkpoint at the current mutation log position.
    pub(super) fn capture(&self, reachable: bool) -> FlowEnvironment {
        FlowEnvironment {
            aliases: self.aliases.clone(),
            states: self.states.clone(),
            reachable,
        }
    }

    /// Restore a previously captured environment by rolling back the mutation
    /// log to the checkpoint that corresponds to the environment.
    pub(super) fn restore(&mut self, environment: FlowEnvironment) -> bool {
        self.aliases = environment.aliases;
        self.states = environment.states;
        self.log = MutationLog::default();
        environment.reachable
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
            aliases: Vec::new(),
            states: Vec::new(),
            reachable: false,
        }
    }

    /// Join two paths, retaining only aliases and requirements proven on both.
    pub(super) fn join(left: &Self, right: &Self) -> Self {
        if !left.is_reachable() {
            return right.clone();
        }
        if !right.is_reachable() {
            return left.clone();
        }
        let aliases = left
            .aliases
            .iter()
            .filter_map(|(binding, object)| {
                let found = right
                    .aliases
                    .binary_search_by_key(binding, |(k, _)| *k)
                    .is_ok_and(|pos| right.aliases[pos].1 == *object);
                found.then_some((*binding, *object))
            })
            .collect();
        let states = left
            .states
            .iter()
            .filter_map(|(key, left_state)| {
                let pos = right.states.binary_search_by_key(key, |(k, _)| *k).ok()?;
                let right_state = &right.states[pos].1;
                let mut state = left_state.clone();
                state.retain_requirement_keys(right_state);
                Some((*key, state))
            })
            .collect();
        Self {
            aliases,
            states,
            reachable: true,
        }
    }

    /// Join all reachable paths, or return unreachable when none survive.
    pub(super) fn join_many(environments: &[Self]) -> Self {
        let Some(first) = environments
            .iter()
            .find(|environment| environment.is_reachable())
        else {
            return Self::unreachable();
        };
        environments
            .iter()
            .filter(|environment| environment.is_reachable())
            .skip(1)
            .fold(first.clone(), |joined, environment| {
                Self::join(&joined, environment)
            })
    }

    /// Whether this snapshot represents a reachable execution path.
    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }
}
