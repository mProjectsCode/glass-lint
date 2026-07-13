//! Value transfer and source matching for object-flow states.

use super::{CallArgInfo, FactId, FactPayload, FlowState, ObjectFlowProjector, ObjectId, ValueId};

#[derive(Debug, Clone)]
pub(super) struct SourceCall {
    /// Rooted or syntactic chain selected as the matcher lookup key.
    pub(super) chain: String,
    /// Effective arguments after any call/apply wrapper has been removed.
    pub(super) args: Vec<CallArgInfo>,
    /// Original fact used for deterministic evidence anchoring.
    pub(super) fact_id: FactId,
    /// Whether the original call had rooted provenance.
    pub(super) rooted: bool,
}

impl SourceCall {
    /// Build the canonical source-call view used by indexing and transfer.
    ///
    /// `.call()` and `.apply()` facts carry both the wrapper syntax and the
    /// effective target invocation. Flow rules match the latter, so an unwrap
    /// replaces both the chain and argument list before source matching.
    pub(super) fn from_fact(fact: &crate::analysis::facts::SemanticFact) -> Option<Self> {
        let FactPayload::Call {
            rooted_chain,
            syntactic_chain,
            callee_name,
            args,
            unwrap,
            ..
        } = &fact.payload
        else {
            return None;
        };
        Self::from_parts(
            fact.id,
            rooted_chain,
            syntactic_chain,
            callee_name,
            args,
            unwrap.as_deref(),
        )
    }

    pub(super) fn from_parts(
        fact_id: FactId,
        rooted_chain: &Option<String>,
        syntactic_chain: &Option<String>,
        callee_name: &Option<String>,
        args: &[CallArgInfo],
        unwrap: Option<&crate::analysis::facts::CallUnwrap>,
    ) -> Option<Self> {
        let (chain, args) = unwrap.map_or_else(
            || {
                (
                    rooted_chain
                        .as_deref()
                        .or(syntactic_chain.as_deref())
                        .or(callee_name.as_deref())
                        .map(str::to_owned),
                    args.to_vec(),
                )
            },
            |unwrap| (Some(unwrap.chain.clone()), unwrap.effective_args.clone()),
        );
        Some(Self {
            chain: chain?,
            args,
            fact_id,
            rooted: rooted_chain.is_some(),
        })
    }
}

impl ObjectFlowProjector<'_, '_> {
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(call) = self.calls_by_result.get(&source).cloned()
            && let Some((object, states)) =
                self.match_source(&call.chain, &call.args, call.fact_id, call.rooted)
        {
            if self.states.len().saturating_add(states.len()) > self.limits.max_states {
                return;
            }
            self.aliases.insert(target, object);
            for state in states {
                self.states.insert((object, state.flow), state);
            }
            return;
        }
        if let Some(object) = self.aliases.get(&source).copied() {
            self.aliases.insert(target, object);
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
        chain: &str,
        args: &[CallArgInfo],
        source_fact: FactId,
        rooted: bool,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let ids = self.flow_index.sources.get(chain)?;
        let matching = ids
            .iter()
            .copied()
            .filter(|id| {
                self.flow_index.get(*id).is_some_and(|flow| {
                    flow.sources.iter().any(|source| {
                        source.member_call == chain
                            && source.provenance.matches_rooted(rooted)
                            && source.arguments.iter().all(|matcher| {
                                args.get(matcher.index)
                                    .is_some_and(|arg| matcher.matcher.matches(arg))
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
