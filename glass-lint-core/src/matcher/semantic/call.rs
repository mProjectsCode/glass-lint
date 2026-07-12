//! Canonical call-site representation.
//!
//! Direct, optional, bound, and callable-transform paths all use this
//! effective argument container.  A missing argument remains an explicit
//! invalid expression, so argument predicates fail closed instead of
//! shifting positions.

use std::borrow::Cow;

use swc_common::DUMMY_SP;
use swc_ecma_ast::{CallExpr, Callee, Expr, ExprOrSpread, OptCall};

#[derive(Clone)]
pub(super) struct ResolvedCall<'a> {
    pub(super) target: Option<&'a Expr>,
    pub(super) receiver: Option<&'a Expr>,
    pub(super) args: Cow<'a, [ExprOrSpread]>,
    pub(super) span: swc_common::Span,
    pub(super) optional: bool,
}

impl<'a> From<&'a CallExpr> for ResolvedCall<'a> {
    fn from(call: &'a CallExpr) -> Self {
        Self {
            target: match &call.callee {
                Callee::Expr(expr) => Some(expr),
                Callee::Super(_) | Callee::Import(_) => None,
            },
            receiver: match &call.callee {
                Callee::Expr(expr) => receiver_expr(expr),
                Callee::Super(_) | Callee::Import(_) => None,
            },
            args: Cow::Borrowed(&call.args),
            span: call.span,
            optional: false,
        }
    }
}

impl<'a> From<&'a OptCall> for ResolvedCall<'a> {
    fn from(call: &'a OptCall) -> Self {
        Self {
            target: Some(&call.callee),
            receiver: receiver_expr(&call.callee),
            args: Cow::Borrowed(&call.args),
            span: call.span,
            optional: true,
        }
    }
}

impl<'a> ResolvedCall<'a> {
    pub(super) fn with_target(
        target: &'a Expr,
        args: Cow<'a, [ExprOrSpread]>,
        span: swc_common::Span,
    ) -> Self {
        Self {
            target: Some(target),
            receiver: receiver_expr(target),
            args,
            span,
            optional: false,
        }
    }

    pub(super) fn with_receiver(mut self, receiver: &'a Expr) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub(super) fn prepend_bound_arguments(
        mut self,
        bound: &[Option<super::scope::BoundArgument>],
    ) -> Self {
        if bound.is_empty() {
            return self;
        }
        let mut args = bound
            .iter()
            .map(|value| ExprOrSpread {
                spread: None,
                expr: Box::new(match value {
                    Some(super::scope::BoundArgument::StaticString(value)) => {
                        Expr::Lit(swc_ecma_ast::Lit::Str(swc_ecma_ast::Str {
                            span: DUMMY_SP,
                            value: value.clone().into(),
                            raw: None,
                        }))
                    }
                    Some(super::scope::BoundArgument::RootedExpression(value)) => {
                        Expr::Ident(value.to_string().into())
                    }
                    None => Expr::Invalid(Default::default()),
                }),
            })
            .collect::<Vec<_>>();
        args.extend(self.args.into_owned());
        self.args = Cow::Owned(args);
        self
    }
}

fn receiver_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Member(member) => Some(&member.obj),
        Expr::OptChain(chain) => match &*chain.base {
            swc_ecma_ast::OptChainBase::Member(member) => Some(&member.obj),
            swc_ecma_ast::OptChainBase::Call(call) => receiver_expr(&call.callee),
        },
        Expr::Paren(paren) => receiver_expr(&paren.expr),
        Expr::Seq(sequence) => sequence.exprs.last().and_then(|expr| receiver_expr(expr)),
        _ => None,
    }
}
