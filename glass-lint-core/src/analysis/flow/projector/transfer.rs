//! Value transfer and source matching for object-flow states.
//!
//! Assignment preserves an object identity only when the source is a known
//! flow result or live alias. Unknown and invalidated values are unbound so
//! later sinks cannot inherit stale state.

use smallvec::SmallVec;

use crate::analysis::{
    flow::{
        effect::CallEffectRef,
        projector::{CallArgInfo, FactId, FlowState, ObjectFlowProjector, ObjectId, ValueId},
    },
    model::flow::FlowId,
};

impl ObjectFlowProjector<'_, '_, '_> {
    /// Transfer a source/result alias into object-flow state.
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(fact_id) = self.calls_by_result.get(&source).copied() {
            let cref = self.stream.call_effect(fact_id);
            if let Some(args) = cref.effective_args()
                && let Some((object, states)) = self.match_source(&cref, args, fact_id)
            {
                if self.flow_state.state_count().saturating_add(states.len())
                    > self.run.limits.state_limit()
                {
                    self.flow_state.mark_state_limit_rejected();
                    return;
                }
                self.bind_value(target, object);
                for state in states {
                    self.flow_state.insert_state(state);
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
        call: &CallEffectRef<'_>,
        args: &[CallArgInfo],
        source_fact: FactId,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let candidates = call
            .global_name()
            .and_then(|name| self.plan.global_source_candidates(name))
            .or_else(|| {
                call.rooted()
                    .then(|| call.chain())
                    .flatten()
                    .and_then(|chain| self.plan.source_candidates(chain))
            })?;
        let matching: SmallVec<[FlowId; 8]> = candidates
            .iter()
            .filter(|candidate| {
                candidate.matches_arguments(|matcher| {
                    args.get(matcher.index()).is_some_and(|arg| {
                        matcher
                            .predicate()
                            .matches(arg, self.names, self.stream.values())
                    })
                })
            })
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
