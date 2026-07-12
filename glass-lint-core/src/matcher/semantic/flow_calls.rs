//! Canonical call extraction and value predicates shared by flow analysis.

use std::borrow::Cow;

use swc_common::{DUMMY_SP, Span};
use swc_ecma_ast::{CallExpr, Callee, Expr, ExprOrSpread, OptCall, OptChainBase};

use super::super::rule::{ArgStringMatcher, FlowValueMatcher};
use super::ast::{member_chain, member_prop_name};
use super::call::ResolvedCall;
use super::resolver::Resolver;

pub(super) fn call_member_chain(call: &CallExpr, resolver: &Resolver) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    member_callee_chain(callee, resolver)
}

pub(super) fn effective_flow_call<'a>(
    call: &'a CallExpr,
    resolver: &Resolver,
) -> Option<(String, ResolvedCall<'a>)> {
    let Callee::Expr(target) = &call.callee else {
        return None;
    };
    effective_flow_call_parts(target, &call.args, call.span, resolver)
}

pub(super) fn effective_opt_flow_call<'a>(
    call: &'a OptCall,
    resolver: &Resolver,
) -> Option<(String, ResolvedCall<'a>)> {
    effective_flow_call_parts(&call.callee, &call.args, call.span, resolver)
}

fn effective_flow_call_parts<'a>(
    target: &'a Expr,
    call_args: &'a [ExprOrSpread],
    span: Span,
    resolver: &Resolver,
) -> Option<(String, ResolvedCall<'a>)> {
    let (target, args, receiver) = match target {
        Expr::Member(member) => match member_prop_name(&member.prop).as_deref() {
            Some("call") if !call_args.is_empty() => (
                &*member.obj,
                Cow::Borrowed(&call_args[1..]),
                call_args.first().map(|argument| &*argument.expr),
            ),
            Some("apply") if call_args.len() >= 2 => {
                let apply_args = &call_args[1].expr;
                if let Expr::Array(array) = &**apply_args {
                    if array.elems.iter().any(|element| {
                        element
                            .as_ref()
                            .is_none_or(|element| element.spread.is_some())
                    }) {
                        return None;
                    }
                    (
                        &*member.obj,
                        Cow::Owned(array.elems.iter().flatten().cloned().collect()),
                        call_args.first().map(|argument| &*argument.expr),
                    )
                } else {
                    let values = resolver.static_string_array_expr(apply_args)?;
                    let args = values
                        .into_iter()
                        .map(|value| ExprOrSpread {
                            spread: None,
                            expr: Box::new(Expr::Lit(swc_ecma_ast::Lit::Str(swc_ecma_ast::Str {
                                span: DUMMY_SP,
                                value: value.into(),
                                raw: None,
                            }))),
                        })
                        .collect();
                    (
                        &*member.obj,
                        Cow::Owned(args),
                        call_args.first().map(|argument| &*argument.expr),
                    )
                }
            }
            _ => (target, Cow::Borrowed(call_args), None),
        },
        _ => (target, Cow::Borrowed(call_args), None),
    };
    let call = ResolvedCall::with_target(target, args, span);
    let call = if let Some(receiver) = receiver {
        call.with_receiver(receiver)
    } else {
        call
    };
    Some((member_callee_chain(target, resolver)?, call))
}

pub(super) fn member_callee_chain(expr: &Expr, resolver: &Resolver) -> Option<String> {
    resolver.rooted_expr_chain(expr).or_else(|| match expr {
        Expr::Member(member) => resolver
            .resolve_member(member)
            .rooted_chain
            .or_else(|| member_chain(member)),
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => resolver
                .resolve_member(member)
                .rooted_chain
                .or_else(|| member_chain(member)),
            OptChainBase::Call(call) => member_callee_chain(&call.callee, resolver),
        },
        Expr::Paren(paren) => member_callee_chain(&paren.expr, resolver),
        Expr::Seq(sequence) => sequence
            .exprs
            .last()
            .and_then(|expr| member_callee_chain(expr, resolver)),
        _ => None,
    })
}

pub(super) fn static_arg_matches(
    matcher: &ArgStringMatcher,
    args: &[ExprOrSpread],
    resolver: &Resolver,
) -> bool {
    args.get(matcher.index).is_some_and(|argument| {
        resolver
            .static_string_expr(&argument.expr)
            .is_some_and(|value| {
                matcher.predicate.as_ref().map_or_else(
                    || matcher.values.is_empty() || matcher.values.contains(&value),
                    |predicate| matches_static_value(predicate, &value),
                )
            })
    })
}

pub(super) fn matches_static_value(matcher: &FlowValueMatcher, value: &str) -> bool {
    match matcher {
        FlowValueMatcher::Any => true,
        FlowValueMatcher::StaticExact(values) => values.iter().any(|expected| expected == value),
        FlowValueMatcher::StaticPrefix(prefixes) => {
            prefixes.iter().any(|prefix| value.starts_with(prefix))
        }
        FlowValueMatcher::StaticContainsAny(markers) => {
            markers.iter().any(|marker| value.contains(marker))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            markers.iter().all(|marker| value.contains(marker))
        }
    }
}

pub(super) fn flow_value_matches(
    matcher: &FlowValueMatcher,
    static_value: Option<&str>,
    allow_dynamic_for_any: bool,
) -> bool {
    match matcher {
        FlowValueMatcher::Any => allow_dynamic_for_any || static_value.is_some(),
        FlowValueMatcher::StaticExact(values) => {
            static_value.is_some_and(|value| values.iter().any(|expected| expected == value))
        }
        FlowValueMatcher::StaticPrefix(prefixes) => static_value
            .is_some_and(|value| prefixes.iter().any(|prefix| value.starts_with(prefix))),
        FlowValueMatcher::StaticContainsAny(markers) => {
            static_value.is_some_and(|value| markers.iter().any(|marker| value.contains(marker)))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            static_value.is_some_and(|value| markers.iter().all(|marker| value.contains(marker)))
        }
    }
}
