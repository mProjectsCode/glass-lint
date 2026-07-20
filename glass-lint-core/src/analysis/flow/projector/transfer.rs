//! Value transfer and source matching for object-flow states.
//!
//! Assignment preserves an object identity only when the source is a known
//! flow result or live alias. Unknown and invalidated values are unbound so
//! later sinks cannot inherit stale state.

use super::{CallArgInfo, FactId, FactPayload, FlowState, ObjectFlowProjector, ObjectId, ValueId};
use crate::analysis::{facts::FactStream, value::NamePath};

#[derive(Debug, Clone)]
pub(super) struct SourceCall {
    /// Rooted or syntactic chain selected as the matcher lookup key.
    chain: NamePath,
    /// Effective arguments after any call/apply wrapper has been removed.
    args: Vec<CallArgInfo>,
    /// Original fact used for deterministic evidence anchoring.
    fact_id: FactId,
    /// Whether the original call had rooted provenance.
    rooted: bool,
}

impl SourceCall {
    /// Build the canonical source-call view used by indexing and transfer.
    ///
    /// `.call()` and `.apply()` facts carry both the wrapper syntax and the
    /// effective target invocation. Flow rules match the latter, so an unwrap
    /// replaces both the chain and argument list before source matching.
    pub(super) fn from_fact(
        fact: &crate::analysis::facts::SemanticFact,
        stream: &FactStream,
    ) -> Option<Self> {
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
            stream.names()?,
            rooted_chain.as_ref(),
            syntactic_chain.as_ref(),
            callee_name.and_then(|id| stream.resolve_name(id)),
            args,
            unwrap.as_deref(),
        )
    }

    /// Build a source-call view from explicit canonical call components.
    pub(super) fn from_parts(
        fact_id: FactId,
        names: &crate::analysis::name::NameTable,
        rooted_chain: Option<&NamePath>,
        syntactic_chain: Option<&crate::analysis::SymbolPath>,
        callee_name: Option<&str>,
        args: &[CallArgInfo],
        unwrap: Option<&crate::analysis::facts::CallUnwrap>,
    ) -> Option<Self> {
        let (chain, args) = unwrap.map_or_else(
            || {
                (
                    rooted_chain
                        .cloned()
                        .or_else(|| {
                            syntactic_chain.and_then(|path| NamePath::from_symbol_path(path, names))
                        })
                        .or_else(|| {
                            callee_name.and_then(|name| {
                                NamePath::from_symbol_path(
                                    &crate::analysis::SymbolPath::from(name),
                                    names,
                                )
                            })
                        }),
                    args.to_vec(),
                )
            },
            |unwrap| {
                (
                    NamePath::from_symbol_path(&unwrap.chain, names),
                    unwrap.effective_args.clone(),
                )
            },
        );
        Some(Self {
            chain: chain?,
            args,
            fact_id,
            rooted: rooted_chain.is_some(),
        })
    }

    pub(super) fn chain(&self) -> &NamePath {
        &self.chain
    }

    pub(super) fn arguments(&self) -> &[CallArgInfo] {
        &self.args
    }

    pub(super) fn event(&self) -> FactId {
        self.fact_id
    }

    pub(super) fn has_rooted_provenance(&self) -> bool {
        self.rooted
    }
}

impl ObjectFlowProjector<'_, '_> {
    /// Transfer a source/result alias into object-flow state.
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(call) = self.calls_by_result.get(&source).cloned()
            && let Some((object, states)) = self.match_source(
                call.chain(),
                call.arguments(),
                call.event(),
                call.has_rooted_provenance(),
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
                                args.get(matcher.index)
                                    .is_some_and(|arg| matcher.matcher.matches(arg, self.names))
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
