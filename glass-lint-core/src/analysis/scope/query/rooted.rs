//! Rooted expression traversal shared by lexical and alias collectors.
//!
//! A rooted chain is returned only for a global, proven alias, or returned
//! object. Calls are transparent only through the supported expression
//! shapes; arbitrary computed or dynamic access returns no chain.

use glass_lint_datastructures::SymbolPath;
use swc_ecma_ast::{Expr, Ident, MemberExpr};

use crate::analysis::{
    scope::{FrozenScopeGraph, frozen_assignments::BindingResolutionStatus},
    syntax::{TransparentTerminal, effective_terminal_expr},
};

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
        let resolution = self.binding_resolution_at(ident.sym.as_ref(), ident.span);
        if let Some(rooted) = self.rooted_witness_path(resolution) {
            return Some(rooted);
        }
        if resolution.status() == BindingResolutionStatus::Absent
            && self.is_global(ident.sym.as_ref())
        {
            Some(ident.sym.as_ref().into())
        } else {
            None
        }
    }

    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<SymbolPath> {
        self.resolve_rooted_member_chain(member)
    }
}

pub(in crate::analysis) fn rooted_expr_chain_with(
    context: &impl RootedExprContext,
    expr: &Expr,
) -> Option<SymbolPath> {
    let terminal = effective_terminal_expr(expr)?;
    match terminal {
        TransparentTerminal::Expr(expr) => match expr {
            Expr::This(_) => Some("this".into()),
            Expr::Ident(ident) => context.rooted_ident_chain(ident),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| rooted_expr_chain_with(context, expr)),
            _ => None,
        },
        TransparentTerminal::Member(member) => context.rooted_member_chain(member),
    }
}

impl FrozenScopeGraph {
    /// Resolve a supported expression shape to a rooted symbol path.
    pub(in crate::analysis) fn rooted_expr_chain(&self, expr: &Expr) -> Option<SymbolPath> {
        rooted_expr_chain_with(self, expr)
    }
}
