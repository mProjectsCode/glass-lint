//! Lifecycle state for one object/flow pair.

use std::collections::BTreeSet;

use swc_common::Span;

use super::super::rule::FlowMatcher;
use super::facts::FactId;
use super::flow_index::FlowId;
use super::value::ObjectId;

#[derive(Debug, Clone)]
pub(super) struct FlowState {
    pub(super) flow: FlowId,
    pub(super) source_fact: FactId,
    pub(super) source_span: Span,
    pub(super) object_id: ObjectId,
    pub(super) requirements: BTreeSet<usize>,
}

pub(super) fn state_is_ready(state: &FlowState, flow: &FlowMatcher) -> bool {
    if flow.all_requirements_required {
        state.requirements.len() == flow.requirements.len()
    } else {
        !state.requirements.is_empty()
    }
}
