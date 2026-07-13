//! Argument predicate evaluation over precomputed fact projections.

use super::*;

impl MatcherFacts {
    pub(in crate::analysis) fn compute_argument_evidence_from_stream(
        &self,
        stream: &FactStream,
        member_argument_matchers: &[(usize, &MemberCallMatcher)],
        call_argument_matchers: &[(usize, &CallMatcher)],
        argument_evidence: &mut [Vec<ApiEvidence>],
    ) {
        for fact in stream.facts() {
            if let FactPayload::Call {
                callee_name,
                call_provenance,
                syntactic_chain,
                rooted_chain,
                module_member,
                args,
                unwrap,
                ..
            } = &fact.payload
            {
                // Member argument predicates use the original args.
                if !member_argument_matchers.is_empty() {
                    self.collect_member_argument_evidence_from_args(
                        member_argument_matchers,
                        argument_evidence,
                        fact.id,
                        fact.span,
                        syntactic_chain.as_deref(),
                        rooted_chain.as_deref(),
                        module_member.as_ref(),
                        args,
                    );
                }

                // Call argument predicates: for .call()/.apply(), use the
                // effective args after unwrapping.
                if !call_argument_matchers.is_empty() {
                    let (effective_args, effective_name, effective_provenance) =
                        if let Some(u) = unwrap {
                            (&u.effective_args, Some(u.chain.as_str()), call_provenance)
                        } else {
                            (args, callee_name.as_deref(), call_provenance)
                        };
                    self.collect_call_argument_evidence_from_args(
                        call_argument_matchers,
                        argument_evidence,
                        fact.id,
                        fact.span,
                        effective_name,
                        effective_provenance,
                        effective_args,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_call_argument_evidence_from_args(
        &self,
        matchers: &[(usize, &CallMatcher)],
        evidence: &mut [Vec<ApiEvidence>],
        event: FactId,
        span: Span,
        callee_name: Option<&str>,
        call_provenance: &SymbolCallProvenance,
        args: &[CallArgInfo],
    ) {
        for (rule_index, matcher) in matchers {
            let matcher = *matcher;
            if matcher.matches_call(callee_name, call_provenance, args) {
                evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol: matcher.evidence_symbol(),
                    count: 1,
                    spans: vec![span],
                    event_ids: vec![event.0],
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_member_argument_evidence_from_args(
        &self,
        matchers: &[(usize, &MemberCallMatcher)],
        evidence: &mut [Vec<ApiEvidence>],
        event: FactId,
        span: Span,
        syntactic_chain: Option<&str>,
        resolved_chain: Option<&str>,
        module_member: Option<&SymbolMemberProvenance>,
        args: &[CallArgInfo],
    ) {
        for (rule_index, matcher) in matchers {
            let matcher = *matcher;
            if matcher.matches_member(syntactic_chain, resolved_chain, module_member, args) {
                let symbol = match matcher.provenance {
                    MemberCallProvenance::Any => matcher.evidence_symbol(),
                    MemberCallProvenance::Rooted | MemberCallProvenance::ModuleNamespace { .. } => {
                        syntactic_chain.unwrap_or(&matcher.chain).to_string()
                    }
                };
                evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol,
                    count: 1,
                    spans: vec![span],
                    event_ids: vec![event.0],
                });
            }
        }
    }
}

impl CallMatcher {
    /// Match a call fact using the matcher’s provenance and argument rules.
    /// The fact already contains all AST-independent projections needed here.
    fn matches_call(
        &self,
        callee_name: Option<&str>,
        call_provenance: &SymbolCallProvenance,
        args: &[CallArgInfo],
    ) -> bool {
        let provenance_matches = match &self.provenance {
            CallProvenance::Any => callee_name.is_some_and(|name| name == self.name),
            CallProvenance::Global => matches!(
                call_provenance,
                SymbolCallProvenance::Global { name } if name == &self.name
            ),
            CallProvenance::ModuleExport { module } => matches!(
                call_provenance,
                SymbolCallProvenance::ModuleExport {
                    module: found_module,
                    export
                } if found_module == module && export == &self.name
            ),
        };
        provenance_matches
            && self.arguments.iter().all(|argument| {
                args.get(argument.index)
                    .is_some_and(|arg| argument.matcher.matches(arg))
            })
    }
}

impl MemberCallMatcher {
    /// Match a member-call fact using syntactic, rooted, or module provenance.
    fn matches_member(
        &self,
        syntactic_chain: Option<&str>,
        resolved_chain: Option<&str>,
        module_member: Option<&SymbolMemberProvenance>,
        args: &[CallArgInfo],
    ) -> bool {
        let provenance_matches = match &self.provenance {
            MemberCallProvenance::Any => {
                syntactic_chain == Some(self.chain.as_str())
                    || resolved_chain == Some(self.chain.as_str())
            }
            MemberCallProvenance::Rooted => resolved_chain
                .map(canonical_rooted_chain)
                .is_some_and(|chain| chain == self.chain),
            MemberCallProvenance::ModuleNamespace { module } => matches!(
                module_member,
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: found_module,
                    member
                }) if found_module == module && member == &self.chain
            ),
        };
        provenance_matches
            && self.arguments.iter().all(|argument| {
                args.get(argument.index)
                    .is_some_and(|arg| argument.matcher.matches(arg))
            })
    }
}
