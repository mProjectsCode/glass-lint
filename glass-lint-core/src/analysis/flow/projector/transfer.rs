//! Value transfer and source matching for object-flow states.
//!
//! Assignment preserves an object identity only when the source is a known
//! flow result or live alias. Unknown and invalidated values are unbound so
//! later sinks cannot inherit stale state.

use crate::analysis::{
    facts::FactStream,
    flow::projector::{
        CallArgInfo, FactId, FactPayload, FlowState, ObjectFlowProjector, ObjectId, ValueId,
    },
    name::NameTable,
    value::NamePath,
};

/// Resolve the effective call chain for a call fact in the projector.
///
/// Includes a callee-name fallback that is not needed by the general
/// [`crate::analysis::flow::effect::CallEffectRef`] view.
pub(super) fn projector_chain(
    stream: &FactStream,
    fact_id: FactId,
    names: &NameTable,
) -> Option<NamePath> {
    let fact = stream.fact(fact_id)?;
    match &fact.payload {
        FactPayload::Call {
            rooted_chain,
            syntactic_path,
            callee_name,
            unwrap,
            ..
        } => unwrap
            .as_deref()
            .and_then(|u| u.chain_path.clone())
            .or_else(|| rooted_chain.clone())
            .or_else(|| syntactic_path.clone())
            .or_else(|| {
                callee_name
                    .and_then(|id| stream.resolve_name(id))
                    .and_then(|name| {
                        NamePath::from_symbol_path(&crate::analysis::SymbolPath::from(name), names)
                    })
            }),
        _ => None,
    }
}

/// Whether the call fact had rooted provenance.
pub(super) fn projector_rooted(stream: &FactStream, fact_id: FactId) -> bool {
    stream.fact(fact_id).is_some_and(|fact| {
        matches!(
            &fact.payload,
            FactPayload::Call {
                rooted_chain: Some(_),
                ..
            }
        )
    })
}

/// Return the effective arguments for a call fact, accounting for
/// `.call()`/`.apply()` unwrapping.
pub(super) fn projector_effective_args(
    fact: &crate::analysis::facts::SemanticFact,
) -> Option<&[CallArgInfo]> {
    match &fact.payload {
        FactPayload::Call { args, unwrap, .. } => Some(
            unwrap
                .as_deref()
                .map_or(args.as_slice(), |u| u.effective_args.as_slice()),
        ),
        _ => None,
    }
}

impl ObjectFlowProjector<'_, '_> {
    /// Transfer a source/result alias into object-flow state.
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(fact_id) = self.calls_by_result.get(&source).copied()
            && let Some(args) = self.stream.call_args_for_event(fact_id)
            && let Some(chain) = projector_chain(self.stream, fact_id, self.names)
            && let Some((object, states)) = self.match_source(
                &chain,
                args,
                fact_id,
                projector_rooted(self.stream, fact_id),
            )
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
        let ids = self.flow_index.source_ids(chain)?;
        let matching = ids
            .iter()
            .copied()
            .filter(|id| {
                self.flow_index.get(*id).is_some_and(|flow| {
                    flow.sources.iter().any(|source| {
                        NamePath::from_symbol_path(&source.member_call, self.names)
                            .is_some_and(|member| member == *chain)
                            && source.provenance.matches_rooted(rooted)
                            && source.arguments.iter().all(|matcher| {
                                args.get(matcher.index()).is_some_and(|arg| {
                                    match self.stream.values() {
                                        Some(values) => {
                                            matcher.matcher().matches(arg, self.names, values)
                                        }
                                        None => false,
                                    }
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
