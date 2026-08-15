use std::hash::{Hash, Hasher};

use super::{
    FactId, FlowId, FlowObjectId, FlowReadiness, LifecycleEvidence, LifecycleRollback,
    RequirementIndex, SinkIndex,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowState {
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
pub struct FlowStateKey {
    object: FlowObjectId,
    flow: FlowId,
}

impl FlowStateKey {
    pub fn new(object: FlowObjectId, flow: FlowId) -> Self {
        Self { object, flow }
    }

    pub fn object(self) -> FlowObjectId {
        self.object
    }

    pub fn flow(self) -> FlowId {
        self.flow
    }
}

impl FlowState {
    pub fn new(flow: FlowId, source_event: FactId, object_id: FlowObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            evidence: LifecycleEvidence::default(),
        }
    }

    pub fn key(&self) -> FlowStateKey {
        FlowStateKey::new(self.object_id, self.flow)
    }

    pub fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub fn object_id(&self) -> FlowObjectId {
        self.object_id
    }

    pub fn source_event(&self) -> FactId {
        self.source_event
    }

    pub fn record_requirement(&mut self, index: RequirementIndex, event: FactId) -> bool {
        self.evidence.record_requirement(index, event)
    }

    pub(crate) fn clear_requirement(
        &mut self,
        index: RequirementIndex,
    ) -> Option<LifecycleRollback<FactId>> {
        self.evidence.clear_requirement(index)
    }

    pub fn remove_requirement_event(&mut self, index: RequirementIndex, event: FactId) -> bool {
        self.evidence.remove_requirement_event(index, &event)
    }

    pub(crate) fn restore_requirement(
        &mut self,
        index: RequirementIndex,
        events: &LifecycleRollback<FactId>,
    ) {
        self.evidence.restore_requirement(index, events);
    }

    pub(in crate::analysis) fn is_ready(&self, readiness: FlowReadiness) -> bool {
        self.evidence.requirements_ready(readiness)
    }

    pub fn record_sink(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.evidence.record_sink(index, event)
    }

    pub fn remove_sink_event(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.evidence.remove_sink_event(index, &event)
    }

    pub(in crate::analysis) fn sinks_ready(&self, readiness: FlowReadiness) -> bool {
        self.evidence.sinks_ready(readiness)
    }

    pub(crate) fn requirement_entries(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, Vec<FactId>)> {
        self.evidence.requirement_entries()
    }

    pub(in crate::analysis) fn first_requirement_events(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, &FactId)> {
        self.evidence.first_requirement_events()
    }

    pub(crate) fn sink_entries(&self) -> impl Iterator<Item = (SinkIndex, Vec<FactId>)> {
        self.evidence.sink_entries()
    }

    pub(in crate::analysis) fn prior_sinks(&self, event: FactId) -> Vec<FactId> {
        self.evidence.prior_sink_events(|sink| *sink == event)
    }
}
