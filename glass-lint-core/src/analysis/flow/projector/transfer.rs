//! Value transfer and source matching for object-flow states.
//!
//! Assignment preserves an object identity only when the source is a known
//! flow result or live alias. Unknown and invalidated values are unbound so
//! later sinks cannot inherit stale state.

use glass_lint_datastructures::NamePath;

use crate::analysis::flow::{
    effect::CallEffectRef,
    projector::{CallArgInfo, FactId, FlowState, ObjectFlowProjector, ObjectId, ValueId},
};

impl ObjectFlowProjector<'_, '_> {
    /// Transfer a source/result alias into object-flow state.
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(fact_id) = self.calls_by_result.get(&source).copied() {
            let cref = CallEffectRef {
                stream: self.stream,
                event: fact_id,
            };
            if let Some(args) = cref.effective_args()
                && let Some(chain) = cref.chain_owned(self.names)
                && let Some((object, states)) =
                    self.match_source(&chain, args, fact_id, cref.rooted())
            {
                if self.flow_state.state_count().saturating_add(states.len())
                    > self.limits.state_limit()
                {
                    return;
                }
                self.flow_state.bind(target, object);
                for state in states {
                    self.flow_state.insert_state(state);
                }
                return;
            }
        }
        if let Some(object) = self.flow_state.object_for(source) {
            self.flow_state.bind(target, object);
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
        chain: &NamePath,
        args: &[CallArgInfo],
        source_fact: FactId,
        rooted: bool,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let ids = self.plan.source_ids(chain)?;
        let matching = ids
            .iter()
            .copied()
            .filter(|id| {
                self.plan.get(*id).is_some_and(|flow| {
                    flow.sources.iter().any(|source| {
                        self.names
                            .lookup_path(&source.member_call)
                            .is_some_and(|member| member == *chain)
                            && source.is_rooted == rooted
                            && source.arguments.iter().all(|matcher| {
                                args.get(matcher.index()).is_some_and(|arg| {
                                    matcher
                                        .matcher()
                                        .matches(arg, self.names, self.stream.values())
                                })
                            })
                    })
                })
            })
            .collect::<Vec<_>>();
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
