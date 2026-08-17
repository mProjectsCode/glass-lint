use glass_lint_datastructures::{HistoryCursor, HistoryTransition, ParentLinkedHistory};

use crate::{
    analysis::{
        facts::FactId,
        model::{
            flow::{FlowState, FlowStateKey, LifecycleRollback, RequirementIndex, SinkIndex},
            value::{FlowObjectId, ValueId},
        },
    },
    api::classification::RuleIndex,
};

/// An inverse delta that can undo one mutation on an alias or state table.
#[derive(Debug, Clone)]
pub(super) enum InverseDelta {
    AliasInsert(ValueId, FlowObjectId),
    AliasUpdate(ValueId, FlowObjectId, FlowObjectId),
    AliasRemove(ValueId, FlowObjectId),
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
}

impl MutationLog {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            history: ParentLinkedHistory::new(),
            budget_exhausted: false,
            limit,
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
        if self.history.len() >= self.limit {
            self.budget_exhausted = true;
            return;
        }
        self.history.record(delta);
    }

    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.history.checkpoint())
    }

    pub(super) fn transition(
        &mut self,
        checkpoint: Checkpoint,
        mut apply: impl FnMut(HistoryTransition, &InverseDelta),
    ) -> bool {
        if self.budget_exhausted {
            return false;
        }
        self.history
            .transition(checkpoint.0, |direction, delta| apply(direction, delta))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ReportEvidenceKey {
    pub(super) rule: RuleIndex,
    pub(super) flow: usize,
    pub(super) object: FlowObjectId,
    pub(super) event: FactId,
}

impl ReportEvidenceKey {
    pub(super) fn new(rule: RuleIndex, flow: usize, object: FlowObjectId, event: FactId) -> Self {
        Self {
            rule,
            flow,
            object,
            event,
        }
    }
}
