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
//! entry. `snapshot()` (a full clone) is used only at join points where the
//! state of one branch must be compared against another.

use hashbrown::HashMap;
use smol_str::SmolStr;
use swc_common::Spanned;
use swc_ecma_ast::{
    CondExpr, DoWhileStmt, ForInStmt, ForOfStmt, ForStmt, IfStmt, SwitchStmt, TryStmt, WhileStmt,
};
use swc_ecma_visit::VisitWith;

use crate::analysis::{
    facts::{ControlKind, ControlRegionId, FactBuilder, FactKind, FactPayload, OriginMap, Span},
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
        let mut cp = self.instance_origins.checkpoint();
        let mut cp_classes = self.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::BranchStart, region);
        stmt.test.visit_with(self);
        self.emit_control(stmt.cons.span(), ControlKind::BranchThen, region);
        stmt.cons.visit_with(self);
        let then_origins = self.instance_origins.snapshot(self.resolver.budget);
        let then_classes = self.class_origins.snapshot(self.resolver.budget);
        self.instance_origins.restore(&cp);
        self.class_origins.restore(&cp_classes);
        if let Some(alt) = &stmt.alt {
            self.emit_control(alt.span(), ControlKind::BranchElse, region);
            alt.visit_with(self);
            self.retain_common_instance_origins(&then_origins);
            self.retain_common_class_origins(&then_classes);
            self.class_origins.commit(&mut cp_classes);
        } else {
            self.class_origins.rollback(&mut cp_classes);
        }
        self.instance_origins.commit(&mut cp);
        self.emit_control(stmt.span(), ControlKind::BranchEnd, region);
    }

    pub(super) fn record_for(&mut self, stmt: &ForStmt) {
        if let Some(init) = &stmt.init {
            init.visit_with(self);
        }
        let mut cp = self.instance_origins.checkpoint();
        let mut cp_classes = self.class_origins.checkpoint();
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
        self.instance_origins.restore(&cp);
        self.class_origins.rollback(&mut cp_classes);
        self.instance_origins.commit(&mut cp);
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
        let mut cp = self.instance_origins.checkpoint();
        let mut cp_classes = self.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(span, ControlKind::LoopStart { guaranteed }, region);
        visit_body(self);
        self.instance_origins.restore(&cp);
        self.class_origins.rollback(&mut cp_classes);
        self.instance_origins.commit(&mut cp);
        self.emit_control(span, ControlKind::LoopEnd, region);
    }

    pub(super) fn record_switch(&mut self, stmt: &SwitchStmt) {
        let mut cp = self.instance_origins.checkpoint();
        let mut cp_classes = self.class_origins.checkpoint();
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
            self.instance_origins.restore(&cp);
        }
        self.instance_origins.commit(&mut cp);
        self.class_origins.rollback(&mut cp_classes);
        self.emit_control(stmt.span(), ControlKind::SwitchEnd, region);
    }

    pub(super) fn record_try(&mut self, stmt: &TryStmt) {
        let mut cp = self.instance_origins.checkpoint();
        let incoming_snapshot = self.instance_origins.snapshot(self.resolver.budget);
        let mut cp_classes = self.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::TryStart, region);
        stmt.block.visit_with(self);
        let try_origins = self.instance_origins.snapshot(self.resolver.budget);
        self.instance_origins.restore(&cp);
        if let Some(handler) = &stmt.handler {
            self.emit_control(handler.span(), ControlKind::CatchStart, region);
            handler.visit_with(self);
            if stmt.finalizer.is_some() {
                let handler_origins =
                    std::mem::take(&mut self.instance_origins).snapshot(self.resolver.budget);
                self.instance_origins = OriginMap::from(try_origins);
                self.retain_common_instance_origins(&handler_origins);
            }
        } else if stmt.finalizer.is_some() {
            self.instance_origins = OriginMap::from(try_origins);
            self.retain_common_instance_origins(&incoming_snapshot);
        }
        if let Some(finalizer) = &stmt.finalizer {
            self.emit_control(finalizer.span(), ControlKind::FinallyStart, region);
            finalizer.visit_with(self);
            self.instance_origins = OriginMap::from(incoming_snapshot);
        }
        self.instance_origins.commit(&mut cp);
        self.class_origins.rollback(&mut cp_classes);
        self.emit_control(stmt.span(), ControlKind::TryEnd, region);
    }

    pub(super) fn record_conditional(&mut self, expr: &CondExpr) {
        let mut cp = self.instance_origins.checkpoint();
        let mut cp_classes = self.class_origins.checkpoint();
        let region = self.next_control_region();
        self.emit_control(expr.span(), ControlKind::BranchStart, region);
        expr.test.visit_with(self);
        self.emit_control(expr.cons.span(), ControlKind::BranchThen, region);
        expr.cons.visit_with(self);
        let then_origins = self.instance_origins.snapshot(self.resolver.budget);
        let then_classes = self.class_origins.snapshot(self.resolver.budget);
        self.instance_origins.restore(&cp);
        self.class_origins.restore(&cp_classes);
        self.emit_control(expr.alt.span(), ControlKind::BranchElse, region);
        expr.alt.visit_with(self);
        self.retain_common_instance_origins(&then_origins);
        self.retain_common_class_origins(&then_classes);
        self.instance_origins.commit(&mut cp);
        self.class_origins.commit(&mut cp_classes);
        self.emit_control(expr.span(), ControlKind::BranchEnd, region);
    }

    fn retain_common_instance_origins(&mut self, other: &HashMap<ValueId, (SmolStr, SmolStr)>) {
        Self::retain_common_origins(&mut self.instance_origins, other, self.resolver.budget);
    }

    fn retain_common_class_origins(&mut self, other: &HashMap<ValueId, (SmolStr, SmolStr)>) {
        Self::retain_common_origins(&mut self.class_origins, other, self.resolver.budget);
    }

    fn retain_common_origins(
        origins: &mut OriginMap<(SmolStr, SmolStr)>,
        other: &HashMap<ValueId, (SmolStr, SmolStr)>,
        budget: &crate::analysis::SemanticBudget,
    ) {
        let to_remove: Vec<ValueId> = origins
            .iter()
            .filter(|(value, origin)| other.get(*value) != Some(*origin))
            .map(|(value, _)| *value)
            .collect();
        for key in to_remove {
            origins.remove(key, budget);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{
        facts::{FactBuilder, FactPayload, Frozen},
        resolution::Resolver,
    };

    fn build_facts(src: &str, filename: &str) -> crate::analysis::facts::FactStream<Frozen> {
        let parsed = crate::parse(src, filename).expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, src);
        let mut builder = FactBuilder::new(&mut resolver);
        swc_ecma_visit::VisitWith::visit_with(&parsed.program, &mut builder);
        builder.into_stream()
    }

    fn count_instance_calls(stream: &crate::analysis::facts::FactStream<Frozen>) -> usize {
        stream
            .facts()
            .iter()
            .filter(|f| {
                matches!(
                    &f.payload,
                    FactPayload::Call {
                        instance_class: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    fn count_method_instance_calls(stream: &crate::analysis::facts::FactStream<Frozen>) -> usize {
        stream
            .facts()
            .iter()
            .filter(|f| {
                matches!(
                    &f.payload,
                    FactPayload::Call {
                        callee_name: Some(name),
                        instance_class: Some(_),
                        ..
                    } if stream.names().resolve(*name) == Some("method")
                )
            })
            .count()
    }

    #[test]
    fn ternary_instance_origins_do_not_cross_incompatible_arms() {
        for source in [
            "import { Foo } from 'a'; import { Bar } from 'b'; let value; flag ? value = new Foo() : value = new Bar(); value.method();",
            "import { Foo } from 'a'; import { Bar } from 'b'; let value; flag ? value = new Bar() : value = new Foo(); value.method();",
        ] {
            let stream = build_facts(source, "ternary-instance.js");
            assert_eq!(
                count_method_instance_calls(&stream),
                0,
                "incompatible ternary arms must not share an instance origin"
            );
        }
    }

    #[test]
    fn ternary_class_origins_do_not_cross_incompatible_arms() {
        for source in [
            "import { Foo } from 'a'; import { Bar } from 'b'; let ctor; flag ? ctor = Foo : ctor = Bar; const value = new ctor(); value.method();",
            "import { Foo } from 'a'; import { Bar } from 'b'; let ctor; flag ? ctor = Bar : ctor = Foo; const value = new ctor(); value.method();",
        ] {
            let stream = build_facts(source, "ternary-class.js");
            assert_eq!(
                count_method_instance_calls(&stream),
                0,
                "incompatible ternary arms must not share a class origin"
            );
        }
    }

    /// Construction in try is visible to a call inside try.
    #[test]
    fn construction_inside_try_is_visible_there() {
        let src = r"
            import { Foo } from 'lib';
            function test() {
                try {
                    let x = new Foo();
                    x.method();
                } catch (e) {}
            }
        ";
        let stream = build_facts(src, "try-inside.js");
        assert!(
            count_instance_calls(&stream) > 0,
            "x.method() after new Foo() inside try should have instance_class"
        );
    }

    /// A value constructed inside try (and copied through a local that the
    /// prepass cannot prove is always constructed) must NOT carry its instance
    /// origin into the catch handler, because the throw may have occurred
    /// before the assignment.
    #[test]
    fn try_origin_does_not_leak_into_catch_handler() {
        let src = r"
            import { Foo } from 'lib';
            function test() {
                let y;
                try {
                    let x = new Foo();
                    y = x;
                } catch (e) {
                    y.method();
                }
            }
        ";
        let stream = build_facts(src, "try-catch-leak.js");
        assert_eq!(
            count_instance_calls(&stream),
            0,
            "y.method() in catch should not see instance origin from try"
        );
    }

    /// A value constructed only in the try path must not carry its instance
    /// origin into the finalizer, because the throw may have prevented the
    /// assignment.
    #[test]
    fn try_only_origin_does_not_leak_into_finally() {
        let src = r"
            import { Foo } from 'lib';
            function test() {
                let y;
                try {
                    let x = new Foo();
                    y = x;
                } catch (e) {
                } finally {
                    y.method();
                }
            }
        ";
        let stream = build_facts(src, "try-only-finally.js");
        assert_eq!(
            count_instance_calls(&stream),
            0,
            "y.method() in finally should not see instance origin from only the try path"
        );
    }

    /// A value constructed before the try/catch retains its instance origin
    /// in the finalizer (it is part of the incoming state).
    #[test]
    fn pre_try_origin_is_visible_in_finally() {
        let src = r"
            import { Foo } from 'lib';
            function test() {
                let y = new Foo();
                try {
                } catch (e) {
                } finally {
                    y.method();
                }
            }
        ";
        let stream = build_facts(src, "pre-try-finally.js");
        assert!(
            count_instance_calls(&stream) > 0,
            "y.method() in finally should see instance origin when y was constructed before try"
        );
    }
}
