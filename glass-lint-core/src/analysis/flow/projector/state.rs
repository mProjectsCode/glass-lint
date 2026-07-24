//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. Joining two
//! reachable environments keeps only equal aliases and common requirement
//! keys, which is the precision boundary that prevents path-local facts from
//! leaking after a control-flow merge.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
};

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

// ---------------------------------------------------------------------------
// Mutation log for checkpoint/rollback
// ---------------------------------------------------------------------------

/// An inverse delta that can undo one mutation on an alias or state table.
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
#[derive(Debug)]
pub(super) struct MutationLog {
    nodes: Vec<LogNode>,
    cursor: usize,
    budget_exhausted: bool,
    limit: usize,
}

impl MutationLog {
    fn new(limit: usize) -> Self {
        Self {
            nodes: Vec::new(),
            cursor: 0,
            budget_exhausted: false,
            limit,
        }
    }

    fn is_budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    fn record(&mut self, delta: InverseDelta) {
        if self.nodes.len() >= self.limit {
            self.budget_exhausted = true;
            return;
        }
        let parent = self.cursor;
        let depth = self.depth(parent) + 1;
        self.nodes.push(LogNode {
            parent,
            depth,
            delta,
        });
        self.cursor = self.nodes.len();
    }

    /// Record a checkpoint at the current log position.
    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.cursor)
    }

    fn transition(
        &mut self,
        checkpoint: Checkpoint,
        aliases: &mut BTreeMap<ValueId, ObjectId>,
        object_refs: &mut BTreeMap<ObjectId, usize>,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) -> bool {
        if checkpoint.0 > self.nodes.len() || self.budget_exhausted {
            return false;
        }
        let mut current = self.cursor;
        let mut target = checkpoint.0;
        while self.depth(current) > self.depth(target) {
            current = self.nodes[current - 1].parent;
        }
        while self.depth(target) > self.depth(current) {
            target = self.nodes[target - 1].parent;
        }
        while current != target {
            current = self.nodes[current - 1].parent;
            target = self.nodes[target - 1].parent;
        }
        let lca = current;
        let mut node = self.cursor;
        while node != lca {
            apply_inverse(&self.nodes[node - 1].delta, aliases, object_refs, states);
            node = self.nodes[node - 1].parent;
        }
        let mut forward = Vec::new();
        node = checkpoint.0;
        while node != lca {
            forward.push(node);
            node = self.nodes[node - 1].parent;
        }
        for node in forward.into_iter().rev() {
            apply_forward(&self.nodes[node - 1].delta, aliases, object_refs, states);
        }
        self.cursor = checkpoint.0;
        true
    }

    fn depth(&self, node: usize) -> usize {
        if node == 0 {
            return 0;
        }
        self.nodes
            .get(node.saturating_sub(1))
            .map_or(0, |entry| entry.depth)
    }
}

fn increment_ref(refs: &mut BTreeMap<ObjectId, usize>, object: ObjectId) {
    *refs.entry(object).or_insert(0) += 1;
}

fn decrement_ref(refs: &mut BTreeMap<ObjectId, usize>, object: ObjectId) {
    if let Some(count) = refs.get_mut(&object) {
        *count -= 1;
        if *count == 0 {
            refs.remove(&object);
        }
    }
}

fn apply_inverse(
    delta: &InverseDelta,
    aliases: &mut BTreeMap<ValueId, ObjectId>,
    object_refs: &mut BTreeMap<ObjectId, usize>,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, _) => {
            if let Some(object) = aliases.remove(value) {
                decrement_ref(object_refs, object);
            }
        }
        InverseDelta::AliasUpdate(value, old, _) => {
            if let Some(prev) = aliases.insert(*value, *old) {
                decrement_ref(object_refs, prev);
                increment_ref(object_refs, *old);
            }
        }
        InverseDelta::AliasRemove(value, object) => {
            aliases.insert(*value, *object);
            increment_ref(object_refs, *object);
        }
        InverseDelta::StateInsert(key, _) => {
            states.remove(key);
        }
        InverseDelta::StateUpdate(key, old, _) => {
            states.insert(*key, old.clone());
        }
        InverseDelta::StateRemove(key, state) => {
            states.insert(*key, state.clone());
        }
    }
}

fn apply_forward(
    delta: &InverseDelta,
    aliases: &mut BTreeMap<ValueId, ObjectId>,
    object_refs: &mut BTreeMap<ObjectId, usize>,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, object) => {
            aliases.insert(*value, *object);
            increment_ref(object_refs, *object);
        }
        InverseDelta::AliasUpdate(value, old, new) => {
            aliases.insert(*value, *new);
            decrement_ref(object_refs, *old);
            increment_ref(object_refs, *new);
        }
        InverseDelta::AliasRemove(value, object) => {
            aliases.remove(value);
            decrement_ref(object_refs, *object);
        }
        InverseDelta::StateInsert(key, state) => {
            states.insert(*key, state.clone());
        }
        InverseDelta::StateUpdate(key, _, new) => {
            states.insert(*key, new.clone());
        }
        InverseDelta::StateRemove(key, _) => {
            states.remove(key);
        }
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

#[derive(Debug)]
/// Mutable live alias and object-state tables for one projector pass.
pub(super) struct FlowStateTable {
    /// Current value aliases, keyed by semantic value identity.
    aliases: BTreeMap<ValueId, ObjectId>,
    /// Reverse index: how many ValueIds alias each ObjectId.
    object_refs: BTreeMap<ObjectId, usize>,
    /// Current lifecycle state for each object and flow matcher.
    states: BTreeMap<FlowStateKey, FlowState>,
    /// Mutation log for checkpoint/rollback.
    log: MutationLog,
    /// Maximum number of state entries allowed.
    state_limit: usize,
}

impl FlowStateTable {
    pub(super) fn new(state_limit: usize, mutation_limit: usize) -> Self {
        Self {
            aliases: BTreeMap::new(),
            object_refs: BTreeMap::new(),
            states: BTreeMap::new(),
            log: MutationLog::new(mutation_limit),
            state_limit,
        }
    }

    pub(super) fn clear(&mut self) {
        let aliases = std::mem::take(&mut self.aliases);
        for (value, object) in aliases {
            self.log.record(InverseDelta::AliasRemove(value, object));
        }
        self.object_refs.clear();
        let states = std::mem::take(&mut self.states);
        for (key, state) in states {
            self.log.record(InverseDelta::StateRemove(key, state));
        }
    }

    pub(super) fn object_for(&self, value: ValueId) -> Option<ObjectId> {
        self.aliases.get(&value).copied()
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.aliases.values().copied()
    }

    pub(super) fn bind(&mut self, value: ValueId, object: ObjectId) {
        if let Some(&old) = self.aliases.get(&value) {
            self.log
                .record(InverseDelta::AliasUpdate(value, old, object));
            self.aliases.insert(value, object);
            decrement_ref(&mut self.object_refs, old);
        } else {
            self.log.record(InverseDelta::AliasInsert(value, object));
            self.aliases.insert(value, object);
        }
        increment_ref(&mut self.object_refs, object);
    }

    pub(super) fn unbind(&mut self, value: ValueId) -> Option<ObjectId> {
        let old_object = self.aliases.remove(&value)?;
        self.log
            .record(InverseDelta::AliasRemove(value, old_object));
        decrement_ref(&mut self.object_refs, old_object);
        Some(old_object)
    }

    pub(super) fn has_alias_for(&self, object: ObjectId) -> bool {
        self.object_refs.contains_key(&object)
    }

    pub(super) fn states_for(
        &self,
        object: ObjectId,
    ) -> impl Iterator<Item = (FlowStateKey, &FlowState)> + '_ {
        self.states
            .iter()
            .filter(move |(key, _)| key.object == object)
            .map(|(key, state)| (*key, state))
    }

    pub(super) fn state(&self, object: ObjectId, flow: FlowId) -> Option<&FlowState> {
        let key = FlowStateKey { object, flow };
        self.states.get(&key)
    }

    pub(super) fn state_mut(&mut self, object: ObjectId, flow: FlowId) -> Option<StateEdit<'_>> {
        let key = FlowStateKey { object, flow };
        let old = self.states.get(&key)?.clone();
        let state_ptr = std::ptr::from_mut(self.states.get_mut(&key).unwrap());
        Some(StateEdit {
            table: self,
            key,
            state_ptr,
            old,
        })
    }

    /// Insert or update a state. Returns `false` when the state limit has been
    /// reached and the insertion was rejected.
    pub(super) fn insert_state(&mut self, state: FlowState) -> bool {
        let key = state.key();
        if let Some(old) = self.states.insert(key, state.clone()) {
            self.log.record(InverseDelta::StateUpdate(key, old, state));
            true
        } else if self.states.len() > self.state_limit {
            self.states.remove(&key);
            false
        } else {
            self.log.record(InverseDelta::StateInsert(key, state));
            true
        }
    }

    pub(super) fn state_count(&self) -> usize {
        self.states.len()
    }

    pub(super) fn mutation_count(&self) -> usize {
        self.log.nodes.len()
    }

    pub(super) fn mutation_exhausted(&self) -> bool {
        self.log.is_budget_exhausted()
    }

    pub(super) fn remove_states_for(&mut self, object: ObjectId) {
        let keys: Vec<FlowStateKey> = self
            .states
            .iter()
            .filter(|(k, _)| k.object == object)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            if let Some(state) = self.states.remove(&key) {
                self.log.record(InverseDelta::StateRemove(key, state));
            }
        }
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
        if self.log.transition(
            environment.checkpoint,
            &mut self.aliases,
            &mut self.object_refs,
            &mut self.states,
        ) {
            environment.reachable
        } else {
            false
        }
    }

    pub(super) fn join_environments(&mut self, environments: &[FlowEnvironment]) -> bool {
        let origin = self.log.checkpoint();
        let mut reachable = environments.iter().filter(|e| e.reachable);

        let Some(first) = reachable.next() else {
            self.clear();
            return false;
        };

        if !self.restore(*first) {
            return false;
        }

        // Compute the intersection of all reachable environments in scratch
        // storage.
        let mut joined_aliases = self.aliases.clone();
        let mut joined_states = self.states.clone();

        for environment in reachable {
            if !self.restore(*environment) {
                return false;
            }
            joined_aliases.retain(|value, object| self.aliases.get(value) == Some(object));
            joined_states.retain(|key, state| {
                self.states.get(key).is_some_and(|other| {
                    state.retain_requirement_keys(other);
                    true
                })
            });
        }

        if !self.restore(FlowEnvironment {
            checkpoint: origin,
            reachable: true,
        }) {
            return false;
        }

        // Replace live tables with the joined result, recording only the net
        // delta between the origin tables and the joined tables. This avoids
        // the old pattern of clear() + bind() / insert_state(), which
        // unconditionally removed every entry and reinserted them through
        // binary-search method calls.
        let old_aliases = std::mem::take(&mut self.aliases);
        let old_states = std::mem::take(&mut self.states);

        merge_delta(
            &old_aliases,
            &joined_aliases,
            &mut self.log,
            &mut self.aliases,
        );
        merge_state_delta(&old_states, &joined_states, &mut self.log, &mut self.states);

        // Rebuild reference counts from the merged alias table.
        self.object_refs.clear();
        for object in self.aliases.values() {
            *self.object_refs.entry(*object).or_insert(0) += 1;
        }

        true
    }
}

pub(super) struct StateEdit<'a> {
    table: &'a mut FlowStateTable,
    key: FlowStateKey,
    state_ptr: *mut FlowState,
    old: FlowState,
}

impl Deref for StateEdit<'_> {
    type Target = FlowState;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.state_ptr }
    }
}

impl DerefMut for StateEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.state_ptr }
    }
}

impl Drop for StateEdit<'_> {
    fn drop(&mut self) {
        let new = unsafe { &*self.state_ptr };
        if *new != self.old {
            self.table.log.record(InverseDelta::StateUpdate(
                self.key,
                self.old.clone(),
                new.clone(),
            ));
        }
    }
}

/// Compute the net delta between `old` and `new` alias maps, writing only
/// the entries that actually changed into the mutation log, and setting `out`
/// to `new`.
fn merge_delta(
    old: &BTreeMap<ValueId, ObjectId>,
    new: &BTreeMap<ValueId, ObjectId>,
    log: &mut MutationLog,
    out: &mut BTreeMap<ValueId, ObjectId>,
) {
    for (value, object) in old {
        match new.get(value) {
            None => log.record(InverseDelta::AliasRemove(*value, *object)),
            Some(new_obj) if new_obj != object => {
                log.record(InverseDelta::AliasUpdate(*value, *object, *new_obj));
            }
            Some(_) => {}
        }
    }
    for (value, object) in new {
        if !old.contains_key(value) {
            log.record(InverseDelta::AliasInsert(*value, *object));
        }
    }
    *out = new.clone();
}

/// Compute the net delta between `old` and `new` state maps.
fn merge_state_delta(
    old: &BTreeMap<FlowStateKey, FlowState>,
    new: &BTreeMap<FlowStateKey, FlowState>,
    log: &mut MutationLog,
    out: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    for (key, state) in old {
        match new.get(key) {
            None => log.record(InverseDelta::StateRemove(*key, state.clone())),
            Some(new_state) if new_state != state => {
                log.record(InverseDelta::StateUpdate(
                    *key,
                    state.clone(),
                    new_state.clone(),
                ));
            }
            Some(_) => {}
        }
    }
    for (key, state) in new {
        if !old.contains_key(key) {
            log.record(InverseDelta::StateInsert(*key, state.clone()));
        }
    }
    *out = new.clone();
}

#[derive(Debug)]
/// Per-rule evidence with a bounded deduplication key set.
///
/// Writes evidence directly into an externally-owned per-rule vec so
/// callers never allocate a second parallel evidence matrix.
pub(super) struct FlowEvidence<'a> {
    /// Evidence grouped by selected rule index, owned by the caller.
    items: &'a mut [Vec<ClassificationEvidence>],
    /// `(rule, flow, object, event)` identities already emitted.
    emitted: BTreeSet<ReportEvidenceKey>,
}

impl<'a> FlowEvidence<'a> {
    pub(super) fn new(evidence: &'a mut [Vec<ClassificationEvidence>]) -> Self {
        Self {
            items: evidence,
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

    pub(super) fn emitted_count(&self) -> usize {
        self.emitted.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_restore_divergent_mutation_paths() {
        let mut table = FlowStateTable::new(262_144, 4096);
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
