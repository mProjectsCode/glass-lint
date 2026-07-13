//! Lifecycle state for one object/flow pair.

use std::collections::BTreeMap;

use super::super::facts::FactId;
use super::super::flow::index::FlowId;
use super::super::value::ObjectId;
use crate::api::compiler::CompiledObjectFlow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlowState {
    pub(super) flow: FlowId,
    pub(super) source_event: FactId,
    pub(super) object_id: ObjectId,
    pub(super) requirements: BTreeMap<usize, FactId>,
}

impl FlowState {
    pub(super) fn new(flow: FlowId, source_event: FactId, object_id: ObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            requirements: BTreeMap::new(),
        }
    }

    pub(super) fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        if flow.all_requirements_required {
            self.requirements.len() == flow.requirements.len()
        } else {
            !self.requirements.is_empty()
        }
    }
}
