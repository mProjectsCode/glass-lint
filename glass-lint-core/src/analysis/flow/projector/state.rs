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
        facts::ControlRegionId,
        flow::projector::history::{
            Checkpoint, InverseDelta, MutationLog, ReportEvidenceKey, decrement_ref, increment_ref,
        },
        model::flow::{FlowId, FlowState, FlowStateKey},
        value::{ObjectId, ValueId},
    },
    api::classification::ClassificationEvidence,
};

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
        self.log.node_count()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::model::fact::FactId, api::classification::RuleIndex};

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

    #[test]
    fn bind_updates_and_unbind_removes_aliases() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId(1), ObjectId(10));
        assert_eq!(table.object_for(ValueId(1)), Some(ObjectId(10)));
        assert!(table.has_alias_for(ObjectId(10)));

        table.bind(ValueId(1), ObjectId(20));
        assert_eq!(table.object_for(ValueId(1)), Some(ObjectId(20)));

        let removed = table.unbind(ValueId(1));
        assert_eq!(removed, Some(ObjectId(20)));
        assert_eq!(table.object_for(ValueId(1)), None);
        assert!(!table.has_alias_for(ObjectId(20)));
    }

    #[test]
    fn object_for_returns_none_for_unbound_value() {
        let table = FlowStateTable::new(100, 100);
        assert_eq!(table.object_for(ValueId(99)), None);
    }

    #[test]
    fn has_alias_for_false_when_no_aliases_exist() {
        let table = FlowStateTable::new(100, 100);
        assert!(!table.has_alias_for(ObjectId(1)));
    }

    #[test]
    fn state_limit_rejects_insertion_beyond_capacity() {
        let mut table = FlowStateTable::new(2, 100);
        let state1 = FlowState::new(FlowId::new(RuleIndex::new(0), 0), FactId(1), ObjectId(1));
        let state2 = FlowState::new(FlowId::new(RuleIndex::new(0), 1), FactId(2), ObjectId(2));
        let state3 = FlowState::new(FlowId::new(RuleIndex::new(0), 2), FactId(3), ObjectId(3));
        assert!(table.insert_state(state1));
        assert!(table.insert_state(state2));
        assert!(!table.insert_state(state3));
        assert_eq!(table.state_count(), 2);
    }

    #[test]
    fn remove_states_for_clears_all_object_states() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId(1), ObjectId(1));
        table.bind(ValueId(2), ObjectId(1));
        let s1 = FlowState::new(FlowId::new(RuleIndex::new(0), 0), FactId(1), ObjectId(1));
        let s2 = FlowState::new(FlowId::new(RuleIndex::new(0), 1), FactId(2), ObjectId(2));
        table.insert_state(s1);
        table.insert_state(s2);
        table.remove_states_for(ObjectId(1));
        assert_eq!(table.states_for(ObjectId(1)).count(), 0);
        assert_eq!(table.state_count(), 1);
    }

    #[test]
    fn join_environments_keeps_only_common_aliases() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId(1), ObjectId(10));
        table.bind(ValueId(2), ObjectId(20));
        let env_a = table.capture(true);

        table.bind(ValueId(2), ObjectId(30));
        table.bind(ValueId(3), ObjectId(40));
        let env_b = table.capture(true);

        table.join_environments(&[env_a, env_b]);
        assert_eq!(table.object_for(ValueId(1)), Some(ObjectId(10)));
        assert_eq!(table.object_for(ValueId(2)), None);
        assert_eq!(table.object_for(ValueId(3)), None);
    }

    #[test]
    fn mutation_count_tracks_mutations() {
        let mut table = FlowStateTable::new(100, 100);
        assert_eq!(table.mutation_count(), 0);
        table.bind(ValueId(1), ObjectId(10));
        assert_eq!(table.mutation_count(), 1);
        table.bind(ValueId(2), ObjectId(20));
        assert_eq!(table.mutation_count(), 2);
        table.unbind(ValueId(1));
        assert_eq!(table.mutation_count(), 3);
    }

    #[test]
    fn clear_removes_all_aliases_and_states() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId(1), ObjectId(10));
        table.bind(ValueId(2), ObjectId(20));
        let s = FlowState::new(FlowId::new(RuleIndex::new(0), 0), FactId(1), ObjectId(10));
        table.insert_state(s);
        table.clear();
        assert_eq!(table.object_for(ValueId(1)), None);
        assert_eq!(table.object_for(ValueId(2)), None);
        assert_eq!(table.state_count(), 0);
    }

    #[test]
    fn state_mut_allows_in_place_update() {
        let mut table = FlowStateTable::new(100, 100);
        let flow = FlowId::new(RuleIndex::new(0), 0);
        let state = FlowState::new(flow, FactId(1), ObjectId(10));
        table.insert_state(state);
        table
            .state_mut(ObjectId(10), flow)
            .unwrap()
            .record_requirement(0, FactId(5));
        let retrieved = table.state(ObjectId(10), flow).unwrap();
        assert_eq!(retrieved.source_event(), FactId(1));
    }
}
