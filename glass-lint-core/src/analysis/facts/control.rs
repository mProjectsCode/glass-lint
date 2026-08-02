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

use smol_str::SmolStr;
use swc_common::Spanned;
use swc_ecma_ast::{
    CondExpr, DoWhileStmt, ForInStmt, ForOfStmt, ForStmt, IfStmt, SwitchStmt, TryStmt, WhileStmt,
};
use swc_ecma_visit::VisitWith;

use crate::analysis::{
    facts::{
        ControlKind, ControlRegionId, FactBuilder, FactKind, FactPayload, OriginSnapshot, Span,
    },
    value::ValueId,
};

impl FactBuilder<'_, '_> {
    /// Allocate the region identity shared by all markers for one construct.
    pub(super) fn next_control_region(&mut self) -> ControlRegionId {
        self.traversal.next_control_region()
    }

    /// Emit a control marker without attaching a speculative value to it.
    pub(super) fn emit_control(&mut self, span: Span, kind: ControlKind, region: ControlRegionId) {
        self.emit(
            FactKind::Control,
            span,
            FactPayload::Control {
                kind,
                region,
                return_value: ValueId::UNKNOWN,
            },
        );
    }

    pub(super) fn record_if(&mut self, stmt: &IfStmt) {
        let mut cp = self.provenance.instance_origins.checkpoint();
        let mut cp_classes = self.provenance.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::BranchStart, region);
        stmt.test.visit_with(self);
        self.emit_control(stmt.cons.span(), ControlKind::BranchThen, region);
        stmt.cons.visit_with(self);
        let then_origins = self
            .provenance
            .instance_origins
            .snapshot(self.resolver.budget);
        let then_classes = self.provenance.class_origins.snapshot(self.resolver.budget);
        self.provenance.instance_origins.restore(&cp);
        self.provenance.class_origins.restore(&cp_classes);
        if let Some(alt) = &stmt.alt {
            self.emit_control(alt.span(), ControlKind::BranchElse, region);
            alt.visit_with(self);
            self.retain_common_instance_origins(&then_origins);
            self.retain_common_class_origins(&then_classes);
            self.provenance.class_origins.commit(&mut cp_classes);
        } else {
            self.provenance.class_origins.rollback(&mut cp_classes);
        }
        self.provenance.instance_origins.commit(&mut cp);
        self.emit_control(stmt.span(), ControlKind::BranchEnd, region);
    }

    pub(super) fn record_for(&mut self, stmt: &ForStmt) {
        if let Some(init) = &stmt.init {
            init.visit_with(self);
        }
        let mut cp = self.provenance.instance_origins.checkpoint();
        let mut cp_classes = self.provenance.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        if let Some(test) = &stmt.test {
            test.visit_with(self);
        }
        stmt.body.visit_with(self);
        if let Some(update) = &stmt.update {
            self.emit_control(stmt.span(), ControlKind::LoopUpdate, region);
            update.visit_with(self);
        }
        self.provenance.instance_origins.restore(&cp);
        self.provenance.class_origins.rollback(&mut cp_classes);
        self.provenance.instance_origins.commit(&mut cp);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
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
        let mut cp = self.provenance.instance_origins.checkpoint();
        let mut cp_classes = self.provenance.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(span, ControlKind::LoopStart { guaranteed }, region);
        visit_body(self);
        self.provenance.instance_origins.restore(&cp);
        self.provenance.class_origins.rollback(&mut cp_classes);
        self.provenance.instance_origins.commit(&mut cp);
        self.emit_control(span, ControlKind::LoopEnd, region);
    }

    pub(super) fn record_switch(&mut self, stmt: &SwitchStmt) {
        let mut cp = self.provenance.instance_origins.checkpoint();
        let mut cp_classes = self.provenance.class_origins.checkpoint();
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
            self.provenance.instance_origins.restore(&cp);
        }
        self.provenance.instance_origins.commit(&mut cp);
        self.provenance.class_origins.rollback(&mut cp_classes);
        self.emit_control(stmt.span(), ControlKind::SwitchEnd, region);
    }

    pub(super) fn record_try(&mut self, stmt: &TryStmt) {
        let mut cp = self.provenance.instance_origins.checkpoint();
        let incoming_snapshot = self
            .provenance
            .instance_origins
            .snapshot(self.resolver.budget);
        let mut cp_classes = self.provenance.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::TryStart, region);
        stmt.block.visit_with(self);
        let try_origins = self
            .provenance
            .instance_origins
            .snapshot(self.resolver.budget);
        self.provenance.instance_origins.restore(&cp);
        if let Some(handler) = &stmt.handler {
            self.emit_control(handler.span(), ControlKind::CatchStart, region);
            handler.visit_with(self);
            if stmt.finalizer.is_some() {
                let handler_origins = self
                    .provenance
                    .instance_origins
                    .snapshot(self.resolver.budget);
                self.provenance.instance_origins.restore_from(try_origins);
                self.provenance
                    .instance_origins
                    .retain_common(&handler_origins, self.resolver.budget);
            }
        } else if stmt.finalizer.is_some() {
            self.provenance.instance_origins.restore_from(try_origins);
            self.provenance
                .instance_origins
                .retain_common(&incoming_snapshot, self.resolver.budget);
        }
        if let Some(finalizer) = &stmt.finalizer {
            self.emit_control(finalizer.span(), ControlKind::FinallyStart, region);
            finalizer.visit_with(self);
            self.provenance
                .instance_origins
                .restore_from(incoming_snapshot);
        }
        self.provenance.instance_origins.commit(&mut cp);
        self.provenance.class_origins.rollback(&mut cp_classes);
        self.emit_control(stmt.span(), ControlKind::TryEnd, region);
    }

    pub(super) fn record_conditional(&mut self, expr: &CondExpr) {
        let mut cp = self.provenance.instance_origins.checkpoint();
        let mut cp_classes = self.provenance.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(expr.span(), ControlKind::BranchStart, region);
        expr.test.visit_with(self);
        self.emit_control(expr.cons.span(), ControlKind::BranchThen, region);
        expr.cons.visit_with(self);
        let then_origins = self
            .provenance
            .instance_origins
            .snapshot(self.resolver.budget);
        let then_classes = self.provenance.class_origins.snapshot(self.resolver.budget);
        self.provenance.instance_origins.restore(&cp);
        self.provenance.class_origins.restore(&cp_classes);
        self.emit_control(expr.alt.span(), ControlKind::BranchElse, region);
        expr.alt.visit_with(self);
        self.retain_common_instance_origins(&then_origins);
        self.retain_common_class_origins(&then_classes);
        self.provenance.instance_origins.commit(&mut cp);
        self.provenance.class_origins.commit(&mut cp_classes);
        self.emit_control(expr.span(), ControlKind::BranchEnd, region);
    }

    fn retain_common_instance_origins(&mut self, other: &OriginSnapshot<(SmolStr, SmolStr)>) {
        self.provenance
            .instance_origins
            .retain_common(other, self.resolver.budget);
    }

    fn retain_common_class_origins(&mut self, other: &OriginSnapshot<(SmolStr, SmolStr)>) {
        self.provenance
            .class_origins
            .retain_common(other, self.resolver.budget);
    }
}
