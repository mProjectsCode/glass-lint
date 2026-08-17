//! Shared syntax-shape normalization for scope provenance.
//!
//! This adapter deliberately stops at syntax. Callers still decide whether a
//! callee, member, or identifier is semantically rooted or lexically valid.

use smol_str::SmolStr;
use swc_ecma_ast::{CallExpr, Expr, Ident, MemberExpr};

use crate::analysis::syntax::{is_dynamic_import, literal_member_property_name};

pub(in crate::analysis) enum ScopeExpression<'a> {
    Ident(&'a Ident),
    Member {
        expression: &'a Expr,
        member: &'a MemberExpr,
        object: &'a Expr,
        literal_property: Option<SmolStr>,
    },
    Call {
        expression: &'a Expr,
        call: &'a CallExpr,
        callee: Option<&'a Expr>,
    },
    OptionalCall {
        callee: &'a Expr,
    },
    Await {
        argument: &'a Expr,
    },
}

/// Normalize the expression wrappers shared by scope provenance paths.
///
/// Parenthesized expressions and the final value of a sequence are
/// transparent. Unsupported expression shapes remain absent so callers fail
/// closed rather than gaining a broader interpretation accidentally.
pub(in crate::analysis) fn normalize_scope_expression(
    expression: &Expr,
) -> Option<ScopeExpression<'_>> {
    let expression = unwrap_scope_expression(expression)?;
    match expression {
        Expr::Ident(ident) => Some(ScopeExpression::Ident(ident)),
        Expr::Member(member) => Some(ScopeExpression::Member {
            expression,
            member,
            object: &member.obj,
            literal_property: literal_member_property_name(&member.prop),
        }),
        Expr::Call(call) => {
            let callee = if is_dynamic_import(&call.callee) {
                None
            } else {
                match &call.callee {
                    swc_ecma_ast::Callee::Expr(callee) => Some(&**callee),
                    swc_ecma_ast::Callee::Super(_) => return None,
                    swc_ecma_ast::Callee::Import(_) => None,
                }
            };
            Some(ScopeExpression::Call {
                expression,
                call,
                callee,
            })
        }
        Expr::OptChain(chain) => {
            let swc_ecma_ast::OptChainBase::Call(call) = &*chain.base else {
                return None;
            };
            let callee = &call.callee;
            Some(ScopeExpression::OptionalCall { callee })
        }
        Expr::Await(await_expr) => Some(ScopeExpression::Await {
            argument: &await_expr.arg,
        }),
        _ => None,
    }
}

fn unwrap_scope_expression(expression: &Expr) -> Option<&Expr> {
    let mut expression = expression;
    loop {
        match expression {
            Expr::Paren(paren) => expression = &paren.expr,
            Expr::Seq(sequence) => expression = sequence.exprs.last()?,
            _ => return Some(expression),
        }
    }
}
