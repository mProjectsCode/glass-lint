//! Rooted expression traversal shared by lexical and alias collectors.
//!
//! A rooted chain is returned only for a global, proven alias, or returned
//! object. Calls are transparent only through the supported expression
//! shapes; arbitrary computed or dynamic access returns no chain.

use glass_lint_datastructures::SymbolPath;
use swc_ecma_ast::{Expr, Ident, MemberExpr, OptChainBase};

use crate::analysis::scope::{BindingProvenance, FrozenScopeGraph};

pub(in crate::analysis) trait RootedExprContext {
    /// Resolve an identifier to a rooted chain at its use position.
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<SymbolPath>;
    /// Resolve a statically named member to a rooted chain.
    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<SymbolPath>;
}

impl RootedExprContext for FrozenScopeGraph {
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<SymbolPath> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        let alternatives = self.binding_alternatives_at(ident.sym.as_ref(), ident.span);
        for provenance in &alternatives {
            let path = match provenance {
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. } => target,
                BindingProvenance::ReturnedObject { source } => source,
                BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }
                | BindingProvenance::ConstructedInstance { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_) => continue,
            };
            if let Some(path) = self.symbol_path(path) {
                return Some(path);
            }
        }
        if alternatives.is_empty() && self.is_global(ident.sym.as_ref()) {
            Some(ident.sym.as_ref().into())
        } else {
            None
        }
    }

    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<SymbolPath> {
        Self::rooted_member_chain(self, member)
    }
}

pub(in crate::analysis) fn rooted_expr_chain_with(
    context: &impl RootedExprContext,
    expr: &Expr,
) -> Option<SymbolPath> {
    match expr {
        Expr::This(_) => Some("this".into()),
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

impl FrozenScopeGraph {
    /// Resolve a supported expression shape to a rooted symbol path.
    pub(in crate::analysis) fn rooted_expr_chain(&self, expr: &Expr) -> Option<SymbolPath> {
        rooted_expr_chain_with(self, expr)
    }
}
