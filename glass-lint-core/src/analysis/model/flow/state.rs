use std::{
    hash::{Hash, Hasher},
    ops::RangeInclusive,
};

use super::{
    FactId, FlowId, FlowObjectId, FlowReadiness, LifecycleEvidence, LifecycleRollback,
    RequirementIndex, SinkIndex,
};
use crate::api::classification::RuleIndex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct FlowState {
    flow: FlowId,
    source_event: FactId,
    object_id: FlowObjectId,
    evidence: LifecycleEvidence<FactId>,
}

impl Hash for FlowState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.flow.hash(state);
        self.source_event.hash(state);
        self.object_id.hash(state);
        self.evidence.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct FlowStateKey {
    object: FlowObjectId,
    flow: FlowId,
}

impl FlowStateKey {
    pub(in crate::analysis) fn new(object: FlowObjectId, flow: FlowId) -> Self {
        Self { object, flow }
    }

    pub(in crate::analysis) fn object(self) -> FlowObjectId {
        self.object
    }

    pub(in crate::analysis) fn flow(self) -> FlowId {
        self.flow
    }

    /// Range of all states for one object in the ordered state table.
    pub(in crate::analysis) fn object_range(object: FlowObjectId) -> RangeInclusive<Self> {
        Self::new(object, FlowId::new(RuleIndex::new(0), 0))
            ..=Self::new(object, FlowId::new(RuleIndex::new(usize::MAX), usize::MAX))
    }
}

impl FlowState {
    pub(in crate::analysis) fn new(
        flow: FlowId,
        source_event: FactId,
        object_id: FlowObjectId,
    ) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            evidence: LifecycleEvidence::default(),
        }
    }

    pub(in crate::analysis) fn key(&self) -> FlowStateKey {
        FlowStateKey::new(self.object_id, self.flow)
    }

    pub(in crate::analysis) fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub(in crate::analysis) fn object_id(&self) -> FlowObjectId {
        self.object_id
    }

    pub(in crate::analysis) fn source_event(&self) -> FactId {
        self.source_event
    }

    pub(in crate::analysis) fn record_requirement(
        &mut self,
        index: RequirementIndex,
        event: FactId,
    ) -> bool {
        self.evidence.record_requirement(index, event)
    }

    pub(in crate::analysis) fn clear_requirement(
        &mut self,
        index: RequirementIndex,
    ) -> Option<LifecycleRollback<FactId>> {
        self.evidence.clear_requirement(index)
    }

    pub(in crate::analysis) fn remove_requirement_event(
        &mut self,
        index: RequirementIndex,
        event: FactId,
    ) -> bool {
        self.evidence.remove_requirement_event(index, &event)
    }

    pub(in crate::analysis) fn restore_requirement(
        &mut self,
        index: RequirementIndex,
        events: &LifecycleRollback<FactId>,
    ) {
        self.evidence.restore_requirement(index, events);
    }

    pub(in crate::analysis) fn is_ready(&self, readiness: FlowReadiness) -> bool {
        self.evidence.requirements_ready(readiness)
    }

    pub(in crate::analysis) fn record_sink(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.evidence.record_sink(index, event)
    }

    pub(in crate::analysis) fn remove_sink_event(
        &mut self,
        index: SinkIndex,
        event: FactId,
    ) -> bool {
        self.evidence.remove_sink_event(index, &event)
    }

    pub(in crate::analysis) fn complete(&self, readiness: FlowReadiness) -> bool {
        self.evidence.complete(readiness)
    }

    pub(in crate::analysis) fn requirement_entries(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, Vec<FactId>)> {
        self.evidence.requirement_entries()
    }

    pub(in crate::analysis) fn first_requirement_events(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, &FactId)> {
        self.evidence.first_requirement_events()
    }

    pub(in crate::analysis) fn sink_entries(
        &self,
    ) -> impl Iterator<Item = (SinkIndex, Vec<FactId>)> {
        self.evidence.sink_entries()
    }

    pub(in crate::analysis) fn prior_sinks(&self, event: FactId) -> Vec<FactId> {
        self.evidence.prior_sink_events(|sink| *sink == event)
    }
}
