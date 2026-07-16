//! Lifecycle state for one object/flow pair.
//!
//! A state records the source event and only the requirements proven for that
//! object. Requirement updates are monotone within a path; control joins may
//! remove path-local keys before the state is used again.

use super::{
    super::{facts::FactId, flow::index::FlowId, value::ObjectId},
    requirements::RequirementSet,
};
use crate::api::compiler::CompiledObjectFlow;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Lifecycle of one allocated object under one selected flow matcher.
pub(super) struct FlowState {
    /// Flow matcher identity owning this state.
    flow: FlowId,
    /// Source event that created the object identity.
    source_event: FactId,
    /// Object identity shared by aliases of the source result.
    object_id: ObjectId,
    /// Requirements proven since the source event.
    requirements: RequirementSet,
}

impl FlowState {
    /// Create an empty lifecycle state for a matched source.
    pub(super) fn new(flow: FlowId, source_event: FactId, object_id: ObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            requirements: RequirementSet::default(),
        }
    }

    /// Whether the flow has enough requirements to emit evidence.
    pub(super) fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        if flow.all_requirements_required {
            self.requirements.len() == flow.requirements.len()
        } else {
            !self.requirements.is_empty()
        }
    }

    /// Return the stable object/flow storage key.
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
