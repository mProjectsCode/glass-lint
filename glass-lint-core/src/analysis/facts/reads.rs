//! Member-read fact projection shared by the visitor and assignment sites.
//!
//! Child visitation stays at each call site so evidence order is unchanged.

use swc_ecma_ast::MemberExpr;

use crate::analysis::facts::{FactBuilder, FactPayload, Spanned};

impl FactBuilder<'_, '_> {
    /// Project one member expression into a `MemberRead` fact.
    pub(super) fn record_member_read(&mut self, member: &MemberExpr) {
        let resolved = self.resolver.resolve_member(member);
        let syntactic_path = self.resolver.syntactic_member_chain(member);
        self.emit(
            member.span(),
            FactPayload::MemberRead {
                syntactic_path,
                rooted_chain: resolved
                    .provenance
                    .rooted_chain
                    .as_ref()
                    .map(|path| self.rooted_name_path(path)),
                module_member: resolved.provenance.module_member.clone(),
                returned_member: Self::returned_path(resolved.provenance.returned_member.as_ref()),
            },
        );
    }
}
