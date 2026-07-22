//! Assignment facts, including conservative invalidation for writes.
//!
//! Every write is represented even when its source is unknown. Flow analysis
//! can then kill stale identities at the write position instead of allowing a
//! value proven before reassignment to leak into later uses.

use swc_ecma_ast::AssignOp;

use crate::analysis::facts::build::{
    AssignExpr, FactBuilder, FactKind, FactPayload, MemberExpr, Pat, Spanned, ValueId, VisitWith,
    member_property_name,
};

impl FactBuilder<'_> {
    /// Emit the write semantics for identifier, member, and destructuring
    /// assignments, including the module-interface consequences of CommonJS
    /// export writes.
    pub(super) fn record_assignment(&mut self, assignment: &AssignExpr) {
        // Any direct write through the CommonJS export objects makes the
        // module interface ambiguous; later project linking must fail closed.
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
        let target = self.resolver.resolve_ident_id(&ident.id);
        self.instance_callables.remove(&target);
        if let Some(callable) = self.instance_callable_for_expr(&assignment.right) {
            self.instance_callables.insert(target, callable);
        }
        self.emit(
            FactKind::Assignment,
            assignment.span(),
            FactPayload::Assignment {
                target,
                source,
                receiver: None,
            },
        );
    }

    fn record_member_assignment(
        &mut self,
        assignment: &AssignExpr,
        member: &MemberExpr,
        _source: ValueId,
    ) {
        // Evaluate the member reference (including computed keys) and the RHS
        // before emitting the write/kill fact.
        member.obj.visit_with(self);
        member.prop.visit_with(self);
        let resolved_member = self.resolver.resolve_member(member);
        let chain = self.resolver.member_expression_chain(member);
        let syntactic_path = chain.as_ref().and_then(|path| self.name_path(path));
        self.emit(
            FactKind::MemberRead,
            member.span(),
            FactPayload::MemberRead {
                syntactic_path,
                rooted_chain: self.rooted_path(resolved_member.rooted_chain.as_ref()),
                module_member: resolved_member.module_member.clone(),
                returned_member: self.returned_path(resolved_member.returned_member.as_ref()),
            },
        );
        assignment.right.visit_with(self);
        let target = resolved_member.id;
        let receiver = self.resolver.resolve_expr_id(&member.obj);
        if assignment.op == AssignOp::Assign {
            let property = self.intern_name(member_property_name(&member.prop).as_deref());
            let value = self.resolver.resolve_expr_id(&assignment.right);
            self.emit(
                FactKind::PropertyWrite,
                assignment.span(),
                FactPayload::PropertyWrite {
                    receiver,
                    property,
                    value,
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
