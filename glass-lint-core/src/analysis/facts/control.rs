//! Control-region markers consumed by bounded flow analysis.
//!
//! A region brackets each branch, loop, switch, or exception path. The
//! projector uses those markers to invalidate or merge state at control-flow
//! joins without unrolling unbounded execution.
//!
//! # Performance note
//!
//! Identity maps use [`OriginMap`] with checkpoint/rollback so that branching
//! only pays for entries actually modified inside a branch, not for every live
//! entry. [`snapshot`](OriginMap::snapshot) (a full clone into an opaque
//! [`OriginSnapshot`]) is used only at join points where the state of one
//! branch must be intersected with another.

use swc_common::Spanned;
use swc_ecma_ast::{
    CondExpr, DoWhileStmt, ForInStmt, ForOfStmt, ForStmt, IfStmt, SwitchStmt, TryStmt, WhileStmt,
};
use swc_ecma_visit::VisitWith;

use crate::analysis::facts::{ControlKind, ControlRegionId, FactBuilder, FactPayload, Span};

impl FactBuilder<'_, '_> {
    /// Allocate the region identity shared by all markers for one construct.
    pub(super) fn next_control_region(&mut self) -> ControlRegionId {
        self.traversal.next_control_region()
    }

    /// Emit a control marker without attaching a speculative value to it.
    pub(super) fn emit_control(&mut self, span: Span, kind: ControlKind, region: ControlRegionId) {
        self.emit(span, FactPayload::Control { kind, region });
    }

    pub(super) fn record_if(&mut self, stmt: &IfStmt) {
        self.record_branch(
            stmt.span(),
            |builder| stmt.test.visit_with(builder),
            stmt.cons.span(),
            |builder| stmt.cons.visit_with(builder),
            stmt.alt.as_ref().map(Spanned::span),
            |builder| {
                if let Some(alt) = &stmt.alt {
                    alt.visit_with(builder);
                }
            },
        );
    }

    pub(super) fn record_for(&mut self, stmt: &ForStmt) {
        if let Some(init) = &stmt.init {
            init.visit_with(self);
        }
        self.record_loop_with_update(
            stmt.span(),
            false,
            |builder| {
                if let Some(test) = &stmt.test {
                    test.visit_with(builder);
                }
                stmt.body.visit_with(builder);
            },
            |builder, region| {
                if let Some(update) = &stmt.update {
                    builder.emit_control(stmt.span(), ControlKind::LoopUpdate, region);
                    update.visit_with(builder);
                }
            },
        );
    }

    pub(super) fn record_for_in(&mut self, stmt: &ForInStmt) {
        self.record_loop(stmt.span(), false, |builder| {
            stmt.left.visit_with(builder);
            stmt.right.visit_with(builder);
            stmt.body.visit_with(builder);
        });
    }

    pub(super) fn record_for_of(&mut self, stmt: &ForOfStmt) {
        self.record_loop(stmt.span(), false, |builder| {
            stmt.left.visit_with(builder);
            stmt.right.visit_with(builder);
            stmt.body.visit_with(builder);
        });
    }

    pub(super) fn record_while(&mut self, stmt: &WhileStmt) {
        self.record_loop(stmt.span(), false, |builder| {
            stmt.test.visit_with(builder);
            stmt.body.visit_with(builder);
        });
    }

    pub(super) fn record_do_while(&mut self, stmt: &DoWhileStmt) {
        self.record_loop(stmt.span(), true, |builder| {
            stmt.body.visit_with(builder);
            stmt.test.visit_with(builder);
        });
    }

    fn record_loop(&mut self, span: Span, guaranteed: bool, visit_body: impl FnOnce(&mut Self)) {
        self.record_loop_with_update(span, guaranteed, visit_body, |_, _| {});
    }

    fn record_loop_with_update(
        &mut self,
        span: Span,
        guaranteed: bool,
        visit_body: impl FnOnce(&mut Self),
        visit_update: impl FnOnce(&mut Self, ControlRegionId),
    ) {
        let mut checkpoint = self.provenance.checkpoint();
        let region = self.next_control_region();
        self.emit_control(span, ControlKind::LoopStart { guaranteed }, region);
        visit_body(self);
        visit_update(self, region);
        self.provenance.finish_control_region(&mut checkpoint);
        self.emit_control(span, ControlKind::LoopEnd, region);
    }

    pub(super) fn record_switch(&mut self, stmt: &SwitchStmt) {
        let mut checkpoint = self.provenance.checkpoint();
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::SwitchStart, region);
        stmt.discriminant.visit_with(self);
        for case in &stmt.cases {
            self.emit_control(
                case.span(),
                ControlKind::SwitchCase {
                    is_default: case.test.is_none(),
                },
                region,
            );
            case.visit_with(self);
            self.provenance.restore_instance_alternative(&checkpoint);
        }
        self.provenance.finish_control_region(&mut checkpoint);
        self.emit_control(stmt.span(), ControlKind::SwitchEnd, region);
    }

    pub(super) fn record_try(&mut self, stmt: &TryStmt) {
        let mut checkpoint = self.provenance.checkpoint();
        let incoming_snapshot = self.provenance.snapshot_instances(self.resolver.budget);
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::TryStart, region);
        stmt.block.visit_with(self);
        let try_origins = self.provenance.snapshot_instances(self.resolver.budget);
        self.provenance.restore_instance_alternative(&checkpoint);
        if let Some(handler) = &stmt.handler {
            self.emit_control(handler.span(), ControlKind::CatchStart, region);
            handler.visit_with(self);
            if stmt.finalizer.is_some() {
                let handler_origins = self.provenance.snapshot_instances(self.resolver.budget);
                self.provenance
                    .restore_instance_snapshot(try_origins, &mut checkpoint);
                self.provenance
                    .retain_common_instance_origins(&handler_origins, self.resolver.budget);
            }
        } else if stmt.finalizer.is_some() {
            self.provenance
                .restore_instance_snapshot(try_origins, &mut checkpoint);
            self.provenance
                .retain_common_instance_origins(&incoming_snapshot, self.resolver.budget);
        }
        if let Some(finalizer) = &stmt.finalizer {
            self.emit_control(finalizer.span(), ControlKind::FinallyStart, region);
            finalizer.visit_with(self);
            self.provenance
                .restore_instance_snapshot(incoming_snapshot, &mut checkpoint);
        }
        self.provenance.finish_control_region(&mut checkpoint);
        self.emit_control(stmt.span(), ControlKind::TryEnd, region);
    }

    pub(super) fn record_conditional(&mut self, expr: &CondExpr) {
        self.record_branch(
            expr.span(),
            |builder| expr.test.visit_with(builder),
            expr.cons.span(),
            |builder| expr.cons.visit_with(builder),
            Some(expr.alt.span()),
            |builder| expr.alt.visit_with(builder),
        );
    }

    fn record_branch(
        &mut self,
        span: Span,
        visit_test: impl FnOnce(&mut Self),
        then_span: Span,
        visit_then: impl FnOnce(&mut Self),
        else_span: Option<Span>,
        visit_else: impl FnOnce(&mut Self),
    ) {
        let mut checkpoint = self.provenance.checkpoint();
        let region = self.next_control_region();
        self.emit_control(span, ControlKind::BranchStart, region);
        visit_test(self);
        self.emit_control(then_span, ControlKind::BranchThen, region);
        visit_then(self);
        let then = self.provenance.branch_provenance(self.resolver.budget);
        self.provenance.restore_branch_entry(&checkpoint);
        if let Some(else_span) = else_span {
            self.emit_control(else_span, ControlKind::BranchElse, region);
            visit_else(self);
            self.provenance
                .finish_branch_with_else(&mut checkpoint, &then, self.resolver.budget);
        } else {
            self.provenance.finish_branch_without_else(&mut checkpoint);
        }
        self.emit_control(span, ControlKind::BranchEnd, region);
    }
}
