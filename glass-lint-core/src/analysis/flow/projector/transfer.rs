//! Value transfer and source matching for object-flow states.
//!
//! Assignment preserves an object identity only when the source is a known
//! flow result or live alias. Unknown and invalidated values are unbound so
//! later sinks cannot inherit stale state.

use smallvec::SmallVec;

use crate::analysis::{
    flow::{
        effect::CallShape,
        planning::FlowMatchView,
        projector::{
            CallArgInfo, FactId, FlowState, ObjectFlowProjector, ObjectId, ValueId,
            state::StateAdmission,
        },
    },
    model::flow::FlowId,
};

impl ObjectFlowProjector<'_, '_, '_> {
    /// Transfer a source/result alias into object-flow state.
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(fact_id) = self.inputs.calls_by_result.get(&source).copied() {
            let cref = self.inputs.stream.call_effect(fact_id);
            if let Some(shape) = cref.shape()
                && let Some((object, states)) =
                    self.match_source(&shape, shape.effective_args(), fact_id)
            {
                let aliases = self.value_aliases(target);
                if matches!(
                    self.flow_state.admit_object(&aliases, object, states),
                    StateAdmission::Admitted
                ) {
                    return;
                }
                return;
            }
        }
        if let Some(object) = self.object_for(source) {
            self.bind_value(target, object);
        } else {
            self.unbind_value(target);
        }
    }

    /// Start every flow whose source matches this canonical call.
    ///
    /// A call can satisfy several flows at once, so one object identity is
    /// shared by all matching states. That lets later aliases preserve the
    /// relationship without duplicating the source event.
    fn match_source(
        &mut self,
        call: &CallShape<'_>,
        args: &[CallArgInfo],
        source_fact: FactId,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let matcher = FlowMatchView::new(self.inputs.names, self.inputs.stream.values());
        let candidates = self.inputs.plan.source_candidates_for_call(call)?;
        let matching: SmallVec<[FlowId; 8]> = candidates
            .iter()
            .filter(|candidate| candidate.matches_call(&matcher, args))
            .map(super::super::planning::BoundSource::flow_id)
            .collect();
        if matching.is_empty() {
            return None;
        }
        let object = self.allocate_object_id()?;
        let states = matching
            .into_iter()
            .map(|flow| FlowState::new(flow, source_fact, object))
            .collect();
        Some((object, states))
    }
}
