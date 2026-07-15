use swc_ecma_ast::AssignOp;

use super::{
    AssignExpr, FactBuilder, FactKind, FactPayload, MemberExpr, Pat, Spanned, ValueId, VisitWith,
    member_prop_name,
};

impl FactBuilder<'_> {
    pub(super) fn record_assignment(&mut self, assignment: &AssignExpr) {
        if assignment.op == AssignOp::Assign
            && let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(
                ident,
            )) = &assignment.left
            && (self
                .resolver
                .is_unshadowed_commonjs_name(&ident.id, "exports")
                || self
                    .resolver
                    .is_unshadowed_commonjs_name(&ident.id, "module"))
        {
            self.interface.mark_unknown_exports();
        }
        self.record_commonjs_export(assignment);
        let source = self.value_for_expr(&assignment.right);
        match &assignment.left {
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(ident)) => {
                self.record_identifier_assignment(assignment, ident, source);
            }
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(
                member,
            )) => self.record_member_assignment(assignment, member, source),
            swc_ecma_ast::AssignTarget::Pat(pattern) => {
                self.record_pattern_assignment(assignment, pattern);
            }
            swc_ecma_ast::AssignTarget::Simple(_) => {}
        }
    }

    fn record_identifier_assignment(
        &mut self,
        assignment: &AssignExpr,
        ident: &swc_ecma_ast::BindingIdent,
        source: ValueId,
    ) {
        assignment.right.visit_with(self);
        self.emit(
            FactKind::Assignment,
            assignment.span(),
            FactPayload::Assignment {
                target: self.resolver.resolve_ident(&ident.id).id,
                source,
                receiver: None,
            },
        );
    }

    fn record_member_assignment(
        &mut self,
        assignment: &AssignExpr,
        member: &MemberExpr,
        source: ValueId,
    ) {
        // Evaluate the member reference (including computed keys) and the RHS
        // before emitting the write/kill fact.
        member.obj.visit_with(self);
        member.prop.visit_with(self);
        let resolved_member = self.resolver.resolve_member(member);
        self.emit(
            FactKind::MemberRead,
            member.span(),
            FactPayload::MemberRead {
                value: resolved_member.id,
                syntactic_chain: self.resolver.member_chain(member),
                rooted_chain: resolved_member.rooted_chain.clone(),
                module_member: resolved_member.module_member.clone(),
                returned_member: resolved_member.returned_member.clone(),
            },
        );
        assignment.right.visit_with(self);
        let target = resolved_member.id;
        let receiver = self.resolver.resolve_expr(&member.obj).id;
        if assignment.op == AssignOp::Assign {
            self.emit(
                FactKind::PropertyWrite,
                assignment.span(),
                FactPayload::PropertyWrite {
                    target,
                    receiver,
                    source,
                    property: member_prop_name(&member.prop),
                    static_value: self.resolver.static_string_expr(&assignment.right),
                },
            );
        } else {
            self.emit(
                FactKind::Assignment,
                assignment.span(),
                FactPayload::Assignment {
                    target,
                    source: ValueId::UNKNOWN,
                    receiver: Some(receiver),
                },
            );
        }
    }

    fn record_pattern_assignment(
        &mut self,
        assignment: &AssignExpr,
        pattern: &swc_ecma_ast::AssignTargetPat,
    ) {
        // Destructuring targets do not have one value identity. Emit
        // conservative writes for each proven target so flow state is
        // invalidated without inventing a shared source value.
        assignment.right.visit_with(self);
        let pattern: Pat = pattern.clone().into();
        let mut targets = Vec::new();
        self.pattern_write_targets(&pattern, &mut targets);
        for (target, receiver) in targets {
            self.emit(
                FactKind::Assignment,
                assignment.span(),
                FactPayload::Assignment {
                    target,
                    source: ValueId::UNKNOWN,
                    receiver,
                },
            );
        }
    }
}
