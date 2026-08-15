use swc_common::Span;

use crate::analysis::scope::build::{ControlFlowFrame, ScopeCollector};

impl ScopeCollector<'_> {
    pub(in crate::analysis::scope::build) fn enter_if(&mut self) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::If {
                incoming,
                consequent: None,
            });
        self.assignment.path.assignment_writes.clear();
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    pub(in crate::analysis::scope::build) fn enter_else(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::If {
                incoming,
                consequent,
            }) = self.assignment.path.control_flow.last_mut()
            else {
                return;
            };
            *consequent = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    pub(in crate::analysis::scope::build) fn exit_if(&mut self, span: Span, has_else: bool) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::If {
            incoming,
            consequent,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let consequent = consequent.unwrap_or_else(|| self.checkpoint());
        let paths = if has_else {
            vec![consequent, self.checkpoint()]
        } else {
            vec![incoming.clone(), consequent]
        };
        self.join_paths(span, &incoming, &paths);
    }

    pub(in crate::analysis::scope::build) fn enter_loop(&mut self, guaranteed: bool) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Loop {
                incoming,
                guaranteed,
                breaks: Vec::new(),
                continues: Vec::new(),
            });
        self.assignment.path.assignment_writes.clear();
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    pub(in crate::analysis::scope::build) fn exit_loop(&mut self, span: Span) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks,
            continues,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let body = self.checkpoint();
        let mut paths = Vec::with_capacity(breaks.len() + 2);
        if !guaranteed {
            paths.push(incoming.clone());
        }
        paths.push(body);
        paths.extend(breaks);
        paths.extend(continues);
        self.join_paths(span, &incoming, &paths);
    }

    pub(in crate::analysis::scope::build) fn enter_switch(&mut self) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Switch {
                incoming,
                cases: Vec::new(),
                breaks: Vec::new(),
            });
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    pub(in crate::analysis::scope::build) fn enter_switch_case(&mut self) {
        let incoming = {
            let Some(ControlFlowFrame::Switch { incoming, .. }) =
                self.assignment.path.control_flow.last()
            else {
                return;
            };
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    pub(in crate::analysis::scope::build) fn exit_switch_case(&mut self) {
        let case = self.checkpoint();
        if let Some(ControlFlowFrame::Switch { cases, .. }) =
            self.assignment.path.control_flow.last_mut()
        {
            cases.push(case);
        }
    }

    pub(in crate::analysis::scope::build) fn exit_switch(&mut self, span: Span) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Switch {
            incoming,
            cases,
            breaks,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let mut paths = Vec::with_capacity(cases.len() + breaks.len() + 1);
        paths.push(incoming.clone());
        paths.extend(cases);
        paths.extend(breaks);
        self.join_paths(span, &incoming, &paths);
    }

    pub(in crate::analysis::scope::build) fn enter_try(
        &mut self,
        has_handler: bool,
        has_finally: bool,
    ) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Try {
                incoming,
                body: None,
                conditional: has_handler || has_finally,
            });
        self.assignment.path.assignment_writes.clear();
        if has_handler || has_finally {
            self.assignment.path.conditional_depth =
                self.assignment.path.conditional_depth.saturating_add(1);
        }
    }

    pub(in crate::analysis::scope::build) fn enter_catch(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::Try { incoming, body, .. }) =
                self.assignment.path.control_flow.last_mut()
            else {
                return;
            };
            *body = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    pub(in crate::analysis::scope::build) fn exit_try(
        &mut self,
        span: Span,
        has_handler: bool,
        has_finally: bool,
    ) {
        let Some(ControlFlowFrame::Try {
            incoming,
            body,
            conditional,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        if conditional {
            self.assignment.path.conditional_depth =
                self.assignment.path.conditional_depth.saturating_sub(1);
        }
        let body = body.unwrap_or_else(|| self.checkpoint());
        let mut paths = Vec::new();
        if has_handler {
            paths.push(body);
            paths.push(self.checkpoint());
        } else if has_finally {
            paths.push(incoming.clone());
            paths.push(body);
        } else {
            paths.push(body);
        }
        self.join_paths(span, &incoming, &paths);
    }

    pub(in crate::analysis::scope::build) fn mark_unreachable(&mut self) {
        self.assignment.path.reachable = false;
    }

    pub(in crate::analysis::scope::build) fn break_exit(&mut self) {
        if self.assignment.path.reachable {
            let checkpoint = self.checkpoint();
            if let Some(frame) = self
                .assignment
                .path
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| {
                    matches!(
                        frame,
                        ControlFlowFrame::Loop { .. } | ControlFlowFrame::Switch { .. }
                    )
                })
            {
                match frame {
                    ControlFlowFrame::Loop { breaks, .. }
                    | ControlFlowFrame::Switch { breaks, .. } => breaks.push(checkpoint),
                    _ => unreachable!("breakable frame was checked above"),
                }
            }
        }
        self.assignment.path.reachable = false;
    }

    pub(in crate::analysis::scope::build) fn continue_exit(&mut self) {
        if self.assignment.path.reachable {
            let checkpoint = self.checkpoint();
            if let Some(ControlFlowFrame::Loop { continues, .. }) = self
                .assignment
                .path
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| matches!(frame, ControlFlowFrame::Loop { .. }))
            {
                continues.push(checkpoint);
            }
        }
        self.assignment.path.reachable = false;
    }

    pub(in crate::analysis::scope::build) fn enter_function(&mut self) {
        let checkpoint = self.checkpoint();
        let control_depth = self.assignment.path.control_flow.len();
        self.assignment
            .path
            .function_checkpoints
            .push(super::super::FunctionCheckpoint {
                checkpoint,
                conditional_depth: self.assignment.path.conditional_depth,
                control_depth,
            });
        self.assignment.path.reachable = true;
        self.assignment.path.assignment_writes.clear();
    }

    pub(in crate::analysis::scope::build) fn exit_function(&mut self) {
        let Some(super::super::FunctionCheckpoint {
            checkpoint,
            conditional_depth,
            control_depth,
        }) = self.assignment.path.function_checkpoints.pop()
        else {
            return;
        };
        self.assignment.path.control_flow.truncate(control_depth);
        self.assignment.path.conditional_depth = conditional_depth;
        self.restore(&checkpoint);
    }
}
