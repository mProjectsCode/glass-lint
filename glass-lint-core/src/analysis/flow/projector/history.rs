use std::collections::BTreeMap;

use glass_lint_datastructures::{HistoryCursor, HistoryTransition, ParentLinkedHistory};

use crate::{
    analysis::{
        facts::FactId,
        flow::projector::state::AliasTable,
        model::flow::{FlowState, FlowStateKey, LifecycleRollback, RequirementIndex, SinkIndex},
        value::{ObjectId, ValueId},
    },
    api::classification::RuleIndex,
};

/// An inverse delta that can undo one mutation on an alias or state table.
#[derive(Debug, Clone)]
pub(super) enum InverseDelta {
    AliasInsert(ValueId, ObjectId),
    AliasUpdate(ValueId, ObjectId, ObjectId),
    AliasRemove(ValueId, ObjectId),
    StateInsert(FlowStateKey, Box<FlowState>),
    StateUpdate(FlowStateKey, Box<FlowState>, Box<FlowState>),
    StateRemove(FlowStateKey, Box<FlowState>),
    RequirementInsert(FlowStateKey, RequirementIndex, FactId),
    RequirementRemove(FlowStateKey, RequirementIndex, LifecycleRollback<FactId>),
    SinkInsert(FlowStateKey, SinkIndex, FactId),
}

/// A position in the persistent mutation history that acts as a checkpoint.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Checkpoint(HistoryCursor);

/// A bounded parent-linked mutation history. Checkpoints are O(1); moving
/// between them applies only the deltas on the paths between the checkpoints.
#[derive(Debug)]
pub(super) struct MutationLog {
    history: ParentLinkedHistory<InverseDelta>,
    budget_exhausted: bool,
    limit: usize,
    /// Total charge count including mutation records and comparison charges.
    /// Used to bound CPU work from join comparisons against the same budget
    /// that bounds mutation-log output.
    charges: usize,
}

impl MutationLog {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            history: ParentLinkedHistory::new(),
            budget_exhausted: false,
            limit,
            charges: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn node_count(&self) -> usize {
        self.history.len()
    }

    pub(super) fn is_budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(super) fn record(&mut self, delta: InverseDelta) {
        if self.charges >= self.limit {
            self.budget_exhausted = true;
            return;
        }
        self.charges += 1;
        self.history.record(delta);
    }

    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.history.checkpoint())
    }

    pub(super) fn transition(
        &mut self,
        checkpoint: Checkpoint,
        aliases: &mut AliasTable,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) -> bool {
        if self.budget_exhausted {
            return false;
        }
        self.history
            .transition(checkpoint.0, |direction, delta| match direction {
                HistoryTransition::Undo => apply_inverse(delta, aliases, states),
                HistoryTransition::Redo => apply_forward(delta, aliases, states),
            })
    }
}

fn apply_inverse(
    delta: &InverseDelta,
    aliases: &mut AliasTable,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, _) => {
            aliases.remove(*value);
        }
        InverseDelta::AliasUpdate(value, old, _) => {
            aliases.set(*value, *old);
        }
        InverseDelta::AliasRemove(value, object) => {
            aliases.set(*value, *object);
        }
        InverseDelta::StateInsert(key, _) => {
            states.remove(key);
        }
        InverseDelta::StateUpdate(key, old, _) => {
            states.insert(*key, (**old).clone());
        }
        InverseDelta::StateRemove(key, state) => {
            states.insert(*key, (**state).clone());
        }
        InverseDelta::RequirementInsert(key, index, event) => {
            if let Some(state) = states.get_mut(key) {
                state.remove_requirement_event(*index, *event);
            }
        }
        InverseDelta::RequirementRemove(key, index, events) => {
            if let Some(state) = states.get_mut(key) {
                state.restore_requirement(*index, events);
            }
        }
        InverseDelta::SinkInsert(key, index, event) => {
            if let Some(state) = states.get_mut(key) {
                state.remove_sink_event(*index, *event);
            }
        }
    }
}

fn apply_forward(
    delta: &InverseDelta,
    aliases: &mut AliasTable,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, object) => {
            aliases.set(*value, *object);
        }
        InverseDelta::AliasUpdate(value, _old, new) => {
            aliases.set(*value, *new);
        }
        InverseDelta::AliasRemove(value, _object) => {
            aliases.remove(*value);
        }
        InverseDelta::StateInsert(key, state) => {
            states.insert(*key, (**state).clone());
        }
        InverseDelta::StateUpdate(key, _, new) => {
            states.insert(*key, (**new).clone());
        }
        InverseDelta::StateRemove(key, _) => {
            states.remove(key);
        }
        InverseDelta::RequirementInsert(key, index, event) => {
            if let Some(state) = states.get_mut(key) {
                state.record_requirement(*index, *event);
            }
        }
        InverseDelta::RequirementRemove(key, index, events) => {
            if let Some(state) = states.get_mut(key) {
                state.clear_requirement(*index);
                state.restore_requirement(*index, events);
            }
        }
        InverseDelta::SinkInsert(key, index, event) => {
            if let Some(state) = states.get_mut(key) {
                state.record_sink(*index, *event);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ReportEvidenceKey {
    pub(super) rule: RuleIndex,
    pub(super) flow: usize,
    pub(super) object: ObjectId,
    pub(super) event: FactId,
}

impl ReportEvidenceKey {
    pub(super) fn new(rule: RuleIndex, flow: usize, object: ObjectId, event: FactId) -> Self {
        Self {
            rule,
            flow,
            object,
            event,
        }
    }
}
