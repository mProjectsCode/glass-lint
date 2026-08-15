//! Member-read fact projection shared by the visitor and assignment sites.
//!
//! Child visitation stays at each call site so evidence order is unchanged.

use swc_ecma_ast::MemberExpr;

use crate::analysis::facts::{FactBuilder, FactPayload, Spanned};

impl FactBuilder<'_, '_> {
    /// Project one member expression into a `MemberRead` fact.
    pub(super) fn record_member_read(&mut self, member: &MemberExpr) {
        let resolved = self.resolver.resolve_member(member);
        let chain = self.resolver.syntactic_member_chain(member);
        let syntactic_path = chain.as_ref().and_then(|path| self.name_path(path));
        self.emit(
            member.span(),
            FactPayload::MemberRead {
                syntactic_path,
                rooted_chain: self.rooted_path(resolved.rooted_chain.as_ref()),
                module_member: resolved.module_member.clone(),
                returned_member: self.returned_path(resolved.returned_member.as_ref()),
            },
        );
    }
}
