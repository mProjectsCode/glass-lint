use super::*;

impl<'rules, 'stream> ObjectFlowProjector<'rules, 'stream> {
    pub(super) fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(call) = self.calls_by_result.get(&source).cloned()
            && let Some((object, states)) =
                self.source_match(&call.chain, &call.args, call.fact_id, call.rooted)
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

    fn source_match(
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
                            && crate::analysis::flow::matcher::member_call_matches_provenance(
                                &source.provenance,
                                rooted,
                            )
                            && source.arguments.iter().all(|matcher| {
                                args.get(matcher.index).is_some_and(|arg| {
                                    crate::analysis::flow::matcher::argument_matches(
                                        &matcher.matcher,
                                        arg,
                                    )
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
            .map(|flow| FlowState {
                flow,
                source_event: source_fact,
                object_id: object,
                requirements: BTreeMap::new(),
            })
            .collect();
        Some((object, states))
    }
}
