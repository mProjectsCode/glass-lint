//! Rooted expression traversal shared by lexical and alias collectors.
//!
//! A rooted chain is returned only for a global, proven alias, or returned
//! object. Calls are transparent only through the supported expression
//! shapes; arbitrary computed or dynamic access returns no chain.

use swc_ecma_ast::{Expr, Ident, MemberExpr, OptChainBase};

use super::super::{BindingProvenance, ScopeGraph};

pub(in crate::analysis) trait RootedExprContext {
    /// Resolve an identifier to a rooted chain at its use position.
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<String>;
    /// Resolve a statically named member to a rooted chain.
    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String>;
}

impl RootedExprContext for ScopeGraph {
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span) {
            None if self.is_global(ident.sym.as_ref()) => Some(ident.sym.to_string()),
            Some(
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. },
            ) => Some(target.to_string()),
            Some(
                BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            )
            | None => None,
            Some(BindingProvenance::ReturnedObject { source }) => Some(source.to_string()),
        }
    }

    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        Self::rooted_member_chain(self, member)
    }
}

pub(in crate::analysis) fn rooted_expr_chain_with(
    context: &impl RootedExprContext,
    expr: &Expr,
) -> Option<String> {
    match expr {
        Expr::This(_) => Some("this".to_string()),
        Expr::Ident(ident) => context.rooted_ident_chain(ident),
        Expr::Member(member) => context.rooted_member_chain(member),
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return None;
            };
            rooted_expr_chain_with(context, callee)
        }
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => context.rooted_member_chain(member),
            OptChainBase::Call(call) => rooted_expr_chain_with(context, &call.callee),
        },
        Expr::Paren(paren) => rooted_expr_chain_with(context, &paren.expr),
        _ => None,
    }
}

impl ScopeGraph {
    /// Resolve a supported expression shape to a rooted symbol path.
    pub(in crate::analysis) fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
        rooted_expr_chain_with(self, expr)
    }
}
