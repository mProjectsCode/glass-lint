//! Assignment facts, including conservative invalidation for writes.
//!
//! Every write is represented even when its source is unknown. Flow analysis
//! can then kill stale identities at the write position instead of allowing a
//! value proven before reassignment to leak into later uses.

use swc_common::Span;
use swc_ecma_ast::AssignOp;

use crate::analysis::facts::{
    AssignExpr, Expr, FactBuilder, FactPayload, MemberExpr, Pat, Spanned, TargetProvenance,
    ValueId, VisitWith, literal_member_property_name,
};

impl FactBuilder<'_, '_> {
    /// Emit the write/kill assignment for update and delete expressions, whose
    /// source value cannot be modeled. A member argument also invalidates the
    /// receiver object; other targets are killed by value identity.
    pub(super) fn emit_member_assignment(&mut self, span: Span, arg: &Expr) {
        let target = self.resolver.resolve_expr_id(arg);
        let receiver = match arg {
            Expr::Member(member) => Some(self.resolver.resolve_expr_id(&member.obj)),
            _ => None,
        };
        self.emit(
            span,
            FactPayload::Assignment {
                target,
                source: ValueId::UNKNOWN,
                receiver,
            },
        );
    }

    /// Emit the write semantics for identifier, member, and destructuring
    /// assignments, including the module-interface consequences of CommonJS
    /// export writes.
    pub(super) fn record_assignment(&mut self, assignment: &AssignExpr) {
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
        let replacement = self.target_provenance(&assignment.right, source);
        self.provenance.replace_targets(
            std::slice::from_ref(&target),
            &replacement,
            self.resolver.budget(),
        );
        self.emit(
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
        self.record_member_read(member);
        assignment.right.visit_with(self);
        let receiver = self.resolver.resolve_expr_id(&member.obj);
        let property_name = literal_member_property_name(&member.prop);
        let property = self.intern_name(property_name.as_deref());
        let value = if assignment.op == AssignOp::Assign {
            self.resolver.resolve_expr_id(&assignment.right)
        } else {
            ValueId::UNKNOWN
        };
        let value_is_precise = assignment.op == AssignOp::Assign;
        let rooted_chain = self.resolver.rooted_write_chain(member);
        self.emit(
            assignment.span(),
            FactPayload::PropertyWrite {
                receiver,
                property,
                rooted_chain: self.rooted_path(rooted_chain.as_ref()),
                value,
                value_is_precise,
            },
        );
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
        let target_values: Vec<ValueId> = targets.iter().map(|(target, _)| *target).collect();
        self.provenance.replace_targets(
            &target_values,
            &TargetProvenance::default(),
            self.resolver.budget(),
        );
        for (target, receiver) in targets {
            self.emit(
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
