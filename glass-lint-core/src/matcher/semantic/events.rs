//! Source ordered semantic events.
//!
//! Some semantic facts are position-sensitive: an assignment affects only
//! later reads. Keeping this ordered log independent of matchers makes that
//! ordering available to all analysis passes without giving each pass its own
//! traversal and sort convention.

use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    AssignExpr, CallExpr, Ident, MemberExpr, NewExpr, OptChainBase, OptChainExpr, Program,
    UnaryExpr, UpdateExpr, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

const MAX_EVENTS: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct EventId(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventKind {
    Declaration,
    Assignment,
    PropertyWrite,
    Call,
    Construction,
    Reference,
    MemberRead,
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
    valid: bool,
}

impl EventLog {
    pub(super) fn collect(program: &Program) -> Self {
        let mut collector = EventCollector::default();
        program.visit_with(&mut collector);
        if collector.events.len() > MAX_EVENTS {
            return Self {
                events: Vec::new(),
                valid: false,
            };
        }
        collector
            .events
            .sort_by_key(|event| (event.span.lo, event.span.hi));
        for (index, event) in collector.events.iter_mut().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            event.id = EventId(index);
        }
        Self {
            events: collector.events,
            valid: true,
        }
    }

    pub(super) fn is_source_ordered(&self) -> bool {
        self.valid
            && self.events.iter().all(|event| {
                event.scope != usize::MAX
                    && matches!(
                        event.kind,
                        EventKind::Declaration
                            | EventKind::Assignment
                            | EventKind::PropertyWrite
                            | EventKind::Call
                            | EventKind::Construction
                            | EventKind::Reference
                            | EventKind::MemberRead
                    )
            })
            && self
                .events
                .windows(2)
                .all(|pair| pair[0].span.lo <= pair[1].span.lo)
    }

    /// Return the stable semantic position for a source span. Synthetic facts
    /// without a dedicated event use the source span as a deterministic
    /// fallback in the evidence layer.
    pub(super) fn order_for(&self, span: Span) -> Option<EventId> {
        self.events
            .iter()
            .find(|event| event.span.lo <= span.lo && event.span.hi >= span.hi)
            .map(|event| event.id)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.events.len()
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
    fn visit_ident(&mut self, ident: &Ident) {
        self.record(EventKind::Reference, ident.span);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        self.record(EventKind::MemberRead, member.span);
        member.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        let kind = match &*chain.base {
            OptChainBase::Call(_) => EventKind::Call,
            OptChainBase::Member(_) => EventKind::MemberRead,
        };
        self.record(kind, chain.span());
        chain.visit_children_with(self);
    }

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

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        self.record(EventKind::Assignment, update.span);
        update.visit_children_with(self);
    }

    fn visit_unary_expr(&mut self, unary: &UnaryExpr) {
        if unary.op == swc_ecma_ast::UnaryOp::Delete {
            self.record(EventKind::PropertyWrite, unary.span);
        }
        unary.visit_children_with(self);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_assigns_stable_source_order_ids() {
        let parsed = crate::parse("const value = make(); value = next();", "events.js")
            .expect("source should parse");
        let log = EventLog::collect(&parsed.program).with_scopes(|_| 0);
        assert!(log.is_source_ordered());
        assert_eq!(log.len(), 8);
        assert!(log.order_for(parsed.program.span()).is_none());
    }
}
