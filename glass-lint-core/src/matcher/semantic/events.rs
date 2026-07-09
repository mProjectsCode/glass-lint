//! Source ordered semantic events.
//!
//! Some semantic facts are position-sensitive: an assignment affects only
//! later reads. Keeping this ordered log independent of matchers makes that
//! ordering available to all analysis passes without giving each pass its own
//! traversal and sort convention.

use swc_common::{Span, Spanned};
use swc_ecma_ast::{AssignExpr, CallExpr, NewExpr, Program, VarDeclarator};
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct EventId(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventKind {
    Declaration,
    Assignment,
    PropertyWrite,
    Call,
    Construction,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Event {
    pub(super) id: EventId,
    pub(super) kind: EventKind,
    pub(super) span: Span,
    pub(super) scope: usize,
}

#[derive(Debug, Default)]
pub(super) struct EventLog {
    events: Vec<Event>,
}

impl EventLog {
    pub(super) fn collect(program: &Program) -> Self {
        let mut collector = EventCollector::default();
        program.visit_with(&mut collector);
        collector
            .events
            .sort_by_key(|event| (event.span.lo, event.span.hi));
        for (index, event) in collector.events.iter_mut().enumerate() {
            event.id = EventId(index as u32);
        }
        Self {
            events: collector.events,
        }
    }

    pub(super) fn is_source_ordered(&self) -> bool {
        self.events.iter().all(|event| {
            event.scope != usize::MAX
                && matches!(
                    event.kind,
                    EventKind::Declaration
                        | EventKind::Assignment
                        | EventKind::PropertyWrite
                        | EventKind::Call
                        | EventKind::Construction
                )
        }) && self
            .events
            .windows(2)
            .all(|pair| pair[0].span.lo <= pair[1].span.lo)
    }

    pub(super) fn with_scopes(mut self, mut scope_for: impl FnMut(Span) -> usize) -> Self {
        // Assign scopes after sorting. Scope lookup is independent of event
        // order, while assigning here makes each event self-contained for
        // consumers that process the log sequentially.
        for event in &mut self.events {
            event.scope = scope_for(event.span);
        }
        self
    }
}

#[derive(Default)]
struct EventCollector {
    events: Vec<Event>,
}

impl EventCollector {
    fn record(&mut self, kind: EventKind, span: Span) {
        self.events.push(Event {
            id: EventId(0),
            kind,
            span,
            scope: usize::MAX,
        });
    }
}

impl Visit for EventCollector {
    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        self.record(EventKind::Declaration, declarator.span());
        declarator.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let kind = if matches!(
            assignment.left,
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(_))
        ) {
            EventKind::PropertyWrite
        } else {
            EventKind::Assignment
        };
        self.record(kind, assignment.span);
        assignment.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record(EventKind::Call, call.span);
        call.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, construction: &NewExpr) {
        self.record(EventKind::Construction, construction.span);
        construction.visit_children_with(self);
    }
}
