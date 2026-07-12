//! Argument predicate evaluation for canonical call facts.

use swc_ecma_ast::{Expr, Ident, ObjectLit};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{CallMatcher, CallProvenance, MemberCallMatcher, MemberCallProvenance};
use super::ast::{SymbolCallProvenance, SymbolMemberProvenance, object_keys};
use super::call::ResolvedCall;
use super::flow_calls::matches_static_value;
use super::resolver::Resolver;
use crate::matcher::rule::canonical_rooted_chain;

pub(super) fn collect_call_argument_evidence(
    resolver: &Resolver,
    matchers: &[(usize, &CallMatcher)],
    evidence: &mut [Vec<ApiEvidence>],
    call: ResolvedCall<'_>,
    ident: &Ident,
    found_provenance: &SymbolCallProvenance,
) {
    debug_assert!(call.target.is_some());
    for (rule_index, matcher) in matchers {
        let matcher = *matcher;
        let call_matches = match &matcher.provenance {
            CallProvenance::Any => ident.sym == *matcher.name,
            CallProvenance::Global => matches!(
                found_provenance,
                SymbolCallProvenance::Global { name } if name == &matcher.name
            ),
            CallProvenance::ModuleExport { module } => matches!(
                found_provenance,
                SymbolCallProvenance::ModuleExport {
                    module: found_module,
                    export
                } if found_module == module && export == &matcher.name
            ),
        };

        if call_matches
            && matcher.arg_strings.iter().all(|arg_matcher| {
                call.args.get(arg_matcher.index).is_some_and(|argument| {
                    resolver
                        .static_string_expr(&argument.expr)
                        .is_some_and(|value| {
                            arg_matcher.predicate.as_ref().map_or_else(
                                || {
                                    arg_matcher.values.is_empty()
                                        || arg_matcher
                                            .values
                                            .iter()
                                            .any(|expected| expected == &value)
                                },
                                |predicate| matches_static_value(predicate, &value),
                            )
                        })
                })
            })
        {
            evidence[*rule_index].push(ApiEvidence {
                kind: ApiMatchKind::CallArgument,
                symbol: matcher.evidence_symbol(),
                count: 1,
                spans: vec![call.span],
            });
        }
    }
}

pub(super) fn collect_member_argument_evidence(
    resolver: &Resolver,
    matchers: &[(usize, &MemberCallMatcher)],
    evidence: &mut [Vec<ApiEvidence>],
    call: ResolvedCall<'_>,
    syntactic_chain: Option<&str>,
    resolved_chain: Option<&str>,
    module_member: Option<&SymbolMemberProvenance>,
) {
    debug_assert!(call.target.is_some());
    for (rule_index, matcher) in matchers {
        let matcher = *matcher;
        let member_matches = match &matcher.provenance {
            MemberCallProvenance::Any => syntactic_chain == Some(&matcher.chain),
            MemberCallProvenance::Rooted => resolved_chain
                .map(canonical_rooted_chain)
                .is_some_and(|chain| chain == matcher.chain),
            MemberCallProvenance::ModuleNamespace { module } => matches!(
                module_member,
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: found_module,
                    member
                }) if found_module == module && member == &matcher.chain
            ),
        };
        if member_matches
            && matcher.arg_strings.iter().all(|arg_matcher| {
                call.args.get(arg_matcher.index).is_some_and(|argument| {
                    resolver
                        .static_string_expr(&argument.expr)
                        .is_some_and(|value| {
                            arg_matcher.predicate.as_ref().map_or_else(
                                || {
                                    arg_matcher.values.is_empty()
                                        || arg_matcher
                                            .values
                                            .iter()
                                            .any(|expected| expected == &value)
                                },
                                |predicate| matches_static_value(predicate, &value),
                            )
                        })
                })
            })
            && matcher.arg_object_keys.iter().all(|key_matcher| {
                call.args
                    .get(key_matcher.index)
                    .and_then(|argument| {
                        resolver
                            .object_keys_expr(&argument.expr)
                            .or_else(|| object_literal(&argument.expr).and_then(object_keys))
                    })
                    .is_some_and(|keys| {
                        key_matcher
                            .keys
                            .iter()
                            .all(|expected| keys.iter().any(|key| key == expected))
                    })
            })
            && matcher.arg_rooted_exprs.iter().all(|root_matcher| {
                call.args
                    .get(root_matcher.index)
                    .and_then(|argument| resolver.rooted_expr_chain(&argument.expr))
                    .map(|chain| canonical_rooted_chain(&chain).to_string())
                    .is_some_and(|chain| {
                        root_matcher
                            .chains
                            .iter()
                            .any(|expected| expected == &chain)
                    })
            })
        {
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
                spans: vec![call.span],
            });
        }
    }
}

fn object_literal(expr: &Expr) -> Option<&ObjectLit> {
    match expr {
        Expr::Object(object) => Some(object),
        Expr::Paren(paren) => object_literal(&paren.expr),
        _ => None,
    }
}
