//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. The projector
//! retains a bounded collection of these checkpoints so aliases and lifecycle
//! requirements stay correlated across control-flow merges.

use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
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

/// Canonical semantic shape of one live flow environment.
///
/// Object ids are projection-local allocation details.  Loop fixed points
/// must compare the aliases and lifecycle states they identify, not the
/// allocation number assigned during a later replay of the same fact slice.
type CanonicalRequirements = Vec<(usize, Vec<crate::analysis::facts::FactId>)>;
type CanonicalFlowState = (
    u32,
    FlowId,
    crate::analysis::facts::FactId,
    CanonicalRequirements,
    CanonicalRequirements,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FlowSemanticSnapshot {
    aliases: Vec<(ValueId, u32)>,
    states: Vec<CanonicalFlowState>,
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

    pub(super) fn record_requirement(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: usize,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey { object, flow };
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        if state.record_requirement(index, event) {
            self.log
                .record(InverseDelta::RequirementInsert(key, index, event));
            true
        } else {
            false
        }
    }

    pub(super) fn clear_requirement(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: usize,
    ) -> bool {
        let key = FlowStateKey { object, flow };
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        let Some(events) = state.clear_requirement(index) else {
            return false;
        };
        self.log
            .record(InverseDelta::RequirementRemove(key, index, events));
        true
    }

    pub(super) fn record_sink(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: usize,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey { object, flow };
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        if state.record_sink(index, event) {
            self.log.record(InverseDelta::SinkInsert(key, index, event));
            true
        } else {
            false
        }
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

    /// Compute a fingerprint (hash) of the current aliases and states without
    /// cloning the full maps.  Used by `join_paths` for O(1) duplicate-path
    /// detection without comparing cloned maps.
    pub(super) fn fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (value, object) in &self.aliases {
            value.hash(&mut hasher);
            object.hash(&mut hasher);
        }
        for (key, state) in &self.states {
            key.hash(&mut hasher);
            state.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Return a deterministic semantic snapshot with object ids normalized to
    /// their first appearance in the alias table.  This intentionally omits
    /// checkpoints and allocation counters so repeated loop iterations can
    /// converge even when they allocate fresh projection-local objects.
    pub(super) fn semantic_snapshot(&self) -> FlowSemanticSnapshot {
        let mut objects = BTreeMap::new();
        let mut next = 0u32;
        for object in self.aliases.values() {
            objects.entry(*object).or_insert_with(|| {
                let id = next;
                next = next.saturating_add(1);
                id
            });
        }
        let aliases = self
            .aliases
            .iter()
            .filter_map(|(value, object)| {
                objects.get(object).copied().map(|object| (*value, object))
            })
            .collect();
        let states = self
            .states
            .iter()
            .filter_map(|(key, state)| {
                // A state without a live alias cannot reach a later transfer.
                // Do not let stale, unreachable state from an overwritten
                // loop binding keep changing the fixed-point shape.
                let object = objects.get(&key.object).copied()?;
                let requirements = state
                    .requirement_keys()
                    .map(|(index, values)| (index, values.iter().copied().collect()))
                    .collect();
                let sinks = state
                    .sink_keys()
                    .map(|(index, values)| (index, values.iter().copied().collect()))
                    .collect();
                Some((object, key.flow, state.source_event(), requirements, sinks))
            })
            .collect();
        FlowSemanticSnapshot { aliases, states }
    }

    #[allow(dead_code)]
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
}

#[derive(Debug)]
/// Per-rule evidence with a bounded deduplication key set.
///
/// Writes evidence directly into an externally-owned per-rule vec so
/// callers never allocate a second parallel evidence matrix.
pub(super) struct FlowEvidence<'a> {
    /// Evidence grouped by selected rule index, owned by the caller.
    items: &'a mut [Vec<ClassificationEvidence>],
    /// `(rule, flow, object, event)` identities with emission count per key.
    /// Multiple traces may be emitted for the same key (e.g., different
    /// requirement events from distinct branches).
    emitted: BTreeMap<ReportEvidenceKey, u32>,
    truncated: BTreeSet<ReportEvidenceKey>,
    /// Maximum evidence items emitted (sum of all counts).
    total_emitted: usize,
}

impl<'a> FlowEvidence<'a> {
    pub(super) fn new(evidence: &'a mut [Vec<ClassificationEvidence>]) -> Self {
        Self {
            items: evidence,
            emitted: BTreeMap::new(),
            truncated: BTreeSet::new(),
            total_emitted: 0,
        }
    }

    /// Reserve a slot for an evidence item. Returns true when the caller
    /// may emit. Allows multiple emissions per key up to `max_per_key`,
    /// and caps the total across all keys to `limit`.
    pub(super) fn try_insert(
        &mut self,
        key: ReportEvidenceKey,
        limit: usize,
        max_per_key: u32,
    ) -> bool {
        let count = self.emitted.entry(key).or_insert(0);
        if *count >= max_per_key {
            self.truncated.insert(key);
            return false;
        }
        if *count == 0 && self.total_emitted >= limit {
            self.truncated.insert(key);
            return false;
        }
        *count += 1;
        self.total_emitted += 1;
        true
    }

    pub(super) fn record(&mut self, rule_index: usize, evidence: ClassificationEvidence) {
        self.items[rule_index].push(evidence);
    }

    pub(super) fn mark_truncated(&mut self) {
        for key in &self.truncated {
            for evidence in &mut self.items[key.rule] {
                if evidence
                    .occurrences
                    .iter()
                    .any(|occurrence| occurrence.fact == Some(key.event.0))
                {
                    evidence.truncated = true;
                }
            }
        }
    }

    pub(super) fn emitted_count(&self) -> usize {
        self.total_emitted
    }
}

#[derive(Debug, Clone)]
/// Saved control construct state used to restore and join environments.
pub(super) enum ControlFrame {
    Branch {
        region: ControlRegionId,
        base: Vec<FlowEnvironment>,
        then_exit: Option<Vec<FlowEnvironment>>,
    },
    Loop {
        region: ControlRegionId,
        body_start: crate::analysis::facts::FactId,
        baseline: Vec<FlowEnvironment>,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: ControlRegionId,
        baseline: Vec<FlowEnvironment>,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: ControlRegionId,
        baseline: Vec<FlowEnvironment>,
        try_exit: Option<Vec<FlowEnvironment>>,
        catch_exit: Option<Vec<FlowEnvironment>>,
        normal_exit: Option<Vec<FlowEnvironment>>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
        normal_count: usize,
    },
    Function {
        caller: Vec<FlowEnvironment>,
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
    pub(super) fn initial() -> Self {
        Self {
            checkpoint: Checkpoint::default(),
            reachable: true,
        }
    }

    /// Whether this snapshot represents a reachable execution path.
    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }

    pub(super) fn reachable(&self) -> bool {
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
    fn fine_grained_state_edits_restore_across_checkpoints() {
        let mut table = FlowStateTable::new(100, 100);
        let flow = FlowId::new(RuleIndex::new(0), 0);
        let state = FlowState::new(flow, FactId(1), ObjectId(10));
        table.insert_state(state);
        let base = table.capture(true);
        assert!(table.record_requirement(ObjectId(10), flow, 0, FactId(5)));
        assert!(table.record_requirement(ObjectId(10), flow, 0, FactId(7)));
        assert!(table.record_sink(ObjectId(10), flow, 0, FactId(6)));
        let retrieved = table.state(ObjectId(10), flow).unwrap();
        assert_eq!(retrieved.source_event(), FactId(1));
        assert_eq!(retrieved.requirement_keys().count(), 1);
        assert_eq!(retrieved.sink_keys().count(), 1);

        let configured = table.capture(true);
        assert!(table.clear_requirement(ObjectId(10), flow, 0));
        assert_eq!(
            table
                .state(ObjectId(10), flow)
                .unwrap()
                .requirement_keys()
                .count(),
            0
        );
        assert!(table.restore(configured));
        let restored = table.state(ObjectId(10), flow).unwrap();
        assert_eq!(restored.requirement_keys().next().unwrap().1.len(), 2);

        assert!(table.restore(base));
        let restored = table.state(ObjectId(10), flow).unwrap();
        assert_eq!(restored.requirement_keys().count(), 0);
        assert_eq!(restored.sink_keys().count(), 0);

        assert!(table.record_requirement(ObjectId(10), flow, 1, FactId(7)));
        assert!(table.restore(base));
        assert_eq!(
            table
                .state(ObjectId(10), flow)
                .unwrap()
                .requirement_keys()
                .count(),
            0
        );
    }
}
