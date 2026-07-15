//! Argument predicate evaluation over precomputed fact projections.

use super::{
    ApiEvidence, ApiMatchKind, CallArgInfo, CallMatcher, CallProvenance, FactId, FactPayload,
    FactStream, MatcherFacts, MemberCallMatcher, MemberCallProvenance, Span, SymbolCallProvenance,
    SymbolMemberProvenance, canonical_rooted_chain,
};

impl MatcherFacts {
    pub(in crate::analysis) fn compute_argument_evidence_from_stream_with_overlay(
        stream: &FactStream,
        member_argument_matchers: &[(usize, &MemberCallMatcher)],
        call_argument_matchers: &[(usize, &CallMatcher)],
        argument_evidence: &mut [Vec<ApiEvidence>],
        identities: Option<
            &std::collections::BTreeMap<(String, String), super::LinkedModuleIdentity>,
        >,
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
                let linked_call_provenance =
                    call_provenance_with_overlay(call_provenance, identities);
                let linked_member_provenance =
                    module_member_with_overlay(module_member.as_ref(), identities);
                // Member argument predicates use the original args.
                if !member_argument_matchers.is_empty() {
                    Self::collect_member_argument_evidence_from_args(
                        member_argument_matchers,
                        argument_evidence,
                        fact.id,
                        fact.span,
                        syntactic_chain.as_deref(),
                        rooted_chain.as_deref(),
                        linked_member_provenance.as_ref(),
                        args,
                    );
                }

                // Call argument predicates: for .call()/.apply(), use the
                // effective args after unwrapping.
                if !call_argument_matchers.is_empty() {
                    let (effective_args, effective_name, effective_provenance) =
                        if let Some(u) = unwrap {
                            (
                                &u.effective_args,
                                Some(u.chain.as_str()),
                                &linked_call_provenance,
                            )
                        } else {
                            (args, callee_name.as_deref(), &linked_call_provenance)
                        };
                    Self::collect_call_argument_evidence_from_args(
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
                    related: Vec::new(),
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_member_argument_evidence_from_args(
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
                    related: Vec::new(),
                });
            }
        }
    }
}

fn call_provenance_with_overlay(
    provenance: &SymbolCallProvenance,
    identities: Option<&std::collections::BTreeMap<(String, String), super::LinkedModuleIdentity>>,
) -> SymbolCallProvenance {
    let Some(identities) = identities else {
        return provenance.clone();
    };
    let SymbolCallProvenance::ModuleExport { module, export } = provenance else {
        return provenance.clone();
    };
    let exact_identity = identities.get(&(module.clone(), export.clone()));
    let identity = exact_identity.or_else(|| identities.get(&(module.clone(), "*".into())));
    match identity {
        Some(super::LinkedModuleIdentity::External {
            module: linked_module,
            export: linked_export,
        }) => SymbolCallProvenance::ModuleExport {
            module: linked_module.clone(),
            export: exact_identity.map_or_else(|| export.clone(), |_| linked_export.clone()),
        },
        Some(super::LinkedModuleIdentity::Global { name }) => {
            SymbolCallProvenance::Global { name: name.clone() }
        }
        Some(super::LinkedModuleIdentity::Qualified | super::LinkedModuleIdentity::Unknown) => {
            SymbolCallProvenance::Local
        }
        None => provenance.clone(),
    }
}

fn module_member_with_overlay(
    provenance: Option<&SymbolMemberProvenance>,
    identities: Option<&std::collections::BTreeMap<(String, String), super::LinkedModuleIdentity>>,
) -> Option<SymbolMemberProvenance> {
    let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = provenance else {
        return provenance.cloned();
    };
    let Some(identities) = identities else {
        return provenance.cloned();
    };
    let identity = identities
        .get(&(module.clone(), member.clone()))
        .or_else(|| identities.get(&(module.clone(), "*".into())));
    match identity {
        Some(super::LinkedModuleIdentity::External { module, .. }) => {
            Some(SymbolMemberProvenance::ModuleNamespace {
                module: module.clone(),
                member: member.clone(),
            })
        }
        Some(
            super::LinkedModuleIdentity::Global { .. }
            | super::LinkedModuleIdentity::Qualified
            | super::LinkedModuleIdentity::Unknown,
        ) => None,
        None => provenance.cloned(),
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
