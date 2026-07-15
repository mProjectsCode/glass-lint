//! Lifecycle state for one object/flow pair.

use super::super::facts::FactId;
use super::super::flow::index::FlowId;
use super::super::value::ObjectId;
use super::requirements::RequirementSet;
use crate::api::compiler::CompiledObjectFlow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlowState {
    flow: FlowId,
    source_event: FactId,
    object_id: ObjectId,
    requirements: RequirementSet,
}

impl FlowState {
    pub(super) fn new(flow: FlowId, source_event: FactId, object_id: ObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            requirements: RequirementSet::default(),
        }
    }

    pub(super) fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        if flow.all_requirements_required {
            self.requirements.len() == flow.requirements.len()
        } else {
            !self.requirements.is_empty()
        }
    }

    pub(super) fn key(&self) -> (ObjectId, FlowId) {
        (self.object_id, self.flow)
    }

    pub(super) fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub(super) fn object_id(&self) -> ObjectId {
        self.object_id
    }

    pub(super) fn source_event(&self) -> FactId {
        self.source_event
    }

    pub(super) fn record_requirement(&mut self, index: usize, event: FactId) {
        self.requirements.insert(index, event);
    }

    pub(super) fn clear_requirement(&mut self, index: usize) {
        self.requirements.remove(index);
    }

    pub(super) fn retain_requirement_keys(&mut self, other: &Self) {
        self.requirements.intersect_keys(&other.requirements);
    }
}
