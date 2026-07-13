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

pub(super) fn state_is_ready(state: &FlowState, flow: &CompiledObjectFlow) -> bool {
    if flow.all_requirements_required {
        state.requirements.len() == flow.requirements.len()
    } else {
        !state.requirements.is_empty()
    }
}
