use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeInclusive,
};

use super::InverseDelta;

mod aliases;

pub(super) use aliases::AliasTable;
pub(in crate::analysis::flow::projector) use updates::PropertyWriteUpdate;

mod updates;

use crate::{
    analysis::{
        facts::FactId,
        flow::projector::history::{Checkpoint, MutationLog},
        model::{
            flow::{FlowId, FlowState, FlowStateKey, RequirementIndex, SinkIndex},
            value::{FlowObjectId, ValueId},
        },
    },
    api::classification::RuleIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// O(1) snapshot of the live tables and reachability at a control boundary.
pub(in crate::analysis::flow::projector) struct FlowEnvironment {
    pub(super) checkpoint: Checkpoint,
    /// Whether execution can reach the snapshot.
    pub(super) reachable: bool,
}

/// Canonical semantic shape of one live flow environment.
///
/// Object ids are projection-local allocation details.  Loop fixed points
/// must compare the aliases and lifecycle states they identify, not the
/// allocation number assigned during a later replay of the same fact slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalObjectId(u32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalAlias {
    value: ValueId,
    object: CanonicalObjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalRequirementState {
    index: RequirementIndex,
    events: Vec<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalSinkState {
    index: SinkIndex,
    events: Vec<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalFlowState {
    object: CanonicalObjectId,
    flow: FlowId,
    source_event: FactId,
    requirements: Vec<CanonicalRequirementState>,
    sinks: Vec<CanonicalSinkState>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::analysis::flow::projector) struct FlowSemanticSnapshot {
    aliases: Vec<CanonicalAlias>,
    states: Vec<CanonicalFlowState>,
}

#[derive(Debug)]
/// Mutable live alias and object-state tables for one projector pass.
pub(in crate::analysis::flow::projector) struct FlowStateTable {
    /// Current value aliases, keyed by semantic value identity.
    aliases: AliasTable,
    /// Current lifecycle state for each object and flow matcher.
    states: BTreeMap<FlowStateKey, FlowState>,
    /// Mutation log for checkpoint/rollback.
    log: MutationLog,
    /// Maximum number of state entries allowed.
    state_limit: usize,
    state_limit_rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Result of admitting one object and its flow-state batch.
pub(in crate::analysis::flow::projector) enum StateAdmission {
    /// Aliases and all batch states were recorded.
    Admitted,
    /// The state limit rejected the batch before any mutation.
    Rejected,
}

impl FlowStateTable {
    pub(in crate::analysis::flow::projector) fn new(
        state_limit: usize,
        mutation_limit: usize,
    ) -> Self {
        Self {
            aliases: AliasTable::default(),
            states: BTreeMap::new(),
            log: MutationLog::new(mutation_limit),
            state_limit,
            state_limit_rejected: false,
        }
    }

    pub(in crate::analysis::flow::projector) fn clear(&mut self) {
        let aliases = self.aliases.take();
        for (value, object) in aliases {
            self.log.record(InverseDelta::AliasRemove(value, object));
        }
        let states = std::mem::take(&mut self.states);
        for (key, state) in states {
            self.log
                .record(InverseDelta::StateRemove(key, Box::new(state)));
        }
    }

    pub(in crate::analysis::flow::projector) fn object_for(
        &self,
        value: ValueId,
    ) -> Option<FlowObjectId> {
        self.aliases.get(value)
    }

    pub(in crate::analysis::flow::projector) fn object_for_any(
        &self,
        values: &[ValueId],
    ) -> Option<FlowObjectId> {
        values.iter().find_map(|value| self.object_for(*value))
    }

    pub(in crate::analysis::flow::projector) fn objects(
        &self,
    ) -> impl Iterator<Item = FlowObjectId> + '_ {
        self.aliases.objects()
    }

    pub(in crate::analysis::flow::projector) fn bind(
        &mut self,
        value: ValueId,
        object: FlowObjectId,
    ) {
        if let Some(old) = self.aliases.set(value, object) {
            self.log
                .record(InverseDelta::AliasUpdate(value, old, object));
        } else {
            self.log.record(InverseDelta::AliasInsert(value, object));
        }
    }

    pub(in crate::analysis::flow::projector) fn unbind(
        &mut self,
        value: ValueId,
    ) -> Option<FlowObjectId> {
        let old_object = self.aliases.remove(value)?;
        self.log
            .record(InverseDelta::AliasRemove(value, old_object));
        Some(old_object)
    }

    pub(in crate::analysis::flow::projector) fn has_alias_for(&self, object: FlowObjectId) -> bool {
        self.aliases.contains_object(object)
    }

    pub(in crate::analysis::flow::projector) fn bind_aliases(
        &mut self,
        values: &[ValueId],
        object: FlowObjectId,
    ) {
        for value in values {
            self.bind(*value, object);
        }
    }

    pub(in crate::analysis::flow::projector) fn unbind_aliases(&mut self, values: &[ValueId]) {
        let mut objects = BTreeSet::new();
        for value in values {
            if let Some(object) = self.unbind(*value) {
                objects.insert(object);
            }
        }
        for object in objects {
            if !self.has_alias_for(object) {
                self.remove_states_for(object);
            }
        }
    }

    pub(in crate::analysis::flow::projector) fn invalidate_aliases(&mut self, values: &[ValueId]) {
        let objects = values
            .iter()
            .filter_map(|value| self.object_for(*value))
            .collect::<BTreeSet<_>>();
        for object in objects {
            self.remove_states_for(object);
        }
    }

    fn object_range(object: FlowObjectId) -> RangeInclusive<FlowStateKey> {
        FlowStateKey::new(object, FlowId::new(RuleIndex::new(0), 0))
            ..=FlowStateKey::new(object, FlowId::new(RuleIndex::new(usize::MAX), usize::MAX))
    }

    pub(in crate::analysis::flow::projector) fn states_for(
        &self,
        object: FlowObjectId,
    ) -> impl Iterator<Item = (FlowStateKey, &FlowState)> + '_ {
        self.states
            .range(Self::object_range(object))
            .map(|(key, state)| (*key, state))
    }

    pub(in crate::analysis::flow::projector) fn state(
        &self,
        object: FlowObjectId,
        flow: FlowId,
    ) -> Option<&FlowState> {
        let key = FlowStateKey::new(object, flow);
        self.states.get(&key)
    }

    pub(in crate::analysis::flow::projector) fn record_requirement(
        &mut self,
        object: FlowObjectId,
        flow: FlowId,
        index: RequirementIndex,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
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

    pub(in crate::analysis::flow::projector) fn clear_requirement(
        &mut self,
        object: FlowObjectId,
        flow: FlowId,
        index: RequirementIndex,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
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

    /// Apply all plan-selected property-write updates for one live object.
    ///
    /// The caller supplies only plan-specific matching results; state-key
    /// traversal and the reversible clear/record mutation protocol remain
    /// owned by this table.
    pub(in crate::analysis::flow::projector) fn apply_property_write(
        &mut self,
        object: FlowObjectId,
        event: FactId,
        mut updates_for: impl FnMut(FlowId) -> Vec<PropertyWriteUpdate>,
    ) -> Vec<FlowId> {
        let flows = self
            .states_for(object)
            .map(|(key, _)| key.flow())
            .collect::<Vec<_>>();
        let mut affected_flows = Vec::new();
        for flow in flows {
            let requirement_updates = updates_for(flow);
            if requirement_updates.is_empty() {
                continue;
            }
            for update in requirement_updates {
                self.clear_requirement(object, flow, update.index);
                if update.value_matches {
                    self.record_requirement(object, flow, update.index, event);
                }
            }
            affected_flows.push(flow);
        }
        affected_flows
    }

    pub(in crate::analysis::flow::projector) fn record_sink(
        &mut self,
        object: FlowObjectId,
        flow: FlowId,
        index: SinkIndex,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
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

    /// Insert or update one state. Returns `false` when the state limit has
    /// been reached and the insertion was rejected.
    #[cfg(test)]
    pub(in crate::analysis::flow::projector) fn insert_state(&mut self, state: FlowState) -> bool {
        let key = state.key();
        if !self.states.contains_key(&key) && self.states.len() >= self.state_limit {
            self.state_limit_rejected = true;
            false
        } else {
            self.insert_state_unchecked(state);
            true
        }
    }

    fn insert_state_unchecked(&mut self, state: FlowState) {
        let key = state.key();
        if let Some(old) = self.states.insert(key, state.clone()) {
            self.log.record(InverseDelta::StateUpdate(
                key,
                Box::new(old),
                Box::new(state),
            ));
        } else {
            self.log
                .record(InverseDelta::StateInsert(key, Box::new(state)));
        }
    }

    /// Admit one object alias and its states as an atomic capacity decision.
    ///
    /// Existing state keys are updates and do not consume capacity. A
    /// rejected batch leaves aliases and states unchanged while recording the
    /// fail-closed state-limit outcome.
    pub(in crate::analysis::flow::projector) fn admit_object(
        &mut self,
        aliases: &[ValueId],
        object: FlowObjectId,
        states: Vec<FlowState>,
    ) -> StateAdmission {
        let mut new_keys = BTreeSet::new();
        for state in &states {
            let key = state.key();
            if !self.states.contains_key(&key) {
                new_keys.insert(key);
            }
        }
        if self.states.len().saturating_add(new_keys.len()) > self.state_limit {
            self.state_limit_rejected = true;
            return StateAdmission::Rejected;
        }
        self.bind_aliases(aliases, object);
        for state in states {
            self.insert_state_unchecked(state);
        }
        StateAdmission::Admitted
    }

    #[cfg(test)]
    pub(in crate::analysis::flow::projector) fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Return a deterministic semantic snapshot with object ids normalized to
    /// their first appearance in the alias table.  This intentionally omits
    /// checkpoints and allocation counters so repeated loop iterations can
    /// converge even when they allocate fresh projection-local objects.
    pub(in crate::analysis::flow::projector) fn semantic_snapshot(&self) -> FlowSemanticSnapshot {
        let mut objects = BTreeMap::new();
        let mut next = 0u32;
        for object in self.aliases.values() {
            objects.entry(*object).or_insert_with(|| {
                let id = CanonicalObjectId(next);
                next = next.saturating_add(1);
                id
            });
        }
        let aliases = self
            .aliases
            .iter()
            .filter_map(|(value, object)| {
                objects.get(object).copied().map(|object| CanonicalAlias {
                    value: *value,
                    object,
                })
            })
            .collect();
        let states = self
            .states
            .iter()
            .filter_map(|(key, state)| {
                // A state without a live alias cannot reach a later transfer.
                // Do not let stale, unreachable state from an overwritten
                // loop binding keep changing the fixed-point shape.
                let object = objects.get(&key.object()).copied()?;
                let requirements = state
                    .requirement_entries()
                    .map(|(index, values)| CanonicalRequirementState {
                        index,
                        events: values,
                    })
                    .collect();
                let sinks = state
                    .sink_entries()
                    .map(|(index, values)| CanonicalSinkState {
                        index,
                        events: values,
                    })
                    .collect();
                Some(CanonicalFlowState {
                    object,
                    flow: key.flow(),
                    source_event: state.source_event(),
                    requirements,
                    sinks,
                })
            })
            .collect();
        FlowSemanticSnapshot { aliases, states }
    }

    #[cfg(test)]
    pub(in crate::analysis::flow::projector) fn mutation_count(&self) -> usize {
        self.log.node_count()
    }

    pub(in crate::analysis::flow::projector) fn mutation_exhausted(&self) -> bool {
        self.log.is_budget_exhausted()
    }

    pub(in crate::analysis::flow::projector) fn state_limit_rejected(&self) -> bool {
        self.state_limit_rejected
    }

    pub(in crate::analysis::flow::projector) fn remove_states_for(&mut self, object: FlowObjectId) {
        let keys: Vec<FlowStateKey> = self
            .states
            .range(Self::object_range(object))
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            if let Some(state) = self.states.remove(&key) {
                self.log
                    .record(InverseDelta::StateRemove(key, Box::new(state)));
            }
        }
    }

    /// Record a checkpoint at the current mutation log position.
    pub(in crate::analysis::flow::projector) fn capture(&self, reachable: bool) -> FlowEnvironment {
        FlowEnvironment {
            checkpoint: self.log.checkpoint(),
            reachable,
        }
    }

    /// Restore a previously captured environment by rolling back the mutation
    /// log to the checkpoint that corresponds to the environment.
    pub(in crate::analysis::flow::projector) fn restore(
        &mut self,
        environment: FlowEnvironment,
    ) -> bool {
        let Self {
            aliases,
            states,
            log,
            ..
        } = self;
        if log.transition(environment.checkpoint, |direction, delta| {
            delta.apply(direction, aliases, states);
        }) {
            environment.reachable
        } else {
            false
        }
    }
}
