use std::collections::BTreeSet;

use swc_common::Span;
use swc_ecma_ast::Expr;

use crate::analysis::{
    scope::{
        BindingProvenance, ProvenanceAlternatives, ScopeId, ScopedName,
        build::{CollectorCheckpoint, ControlFlowFrame, ScopeCollector},
        query::rooted::rooted_expr_chain_with,
    },
    syntax::member_root_identifier,
    value::BindingVersion,
};

impl ScopeCollector<'_> {
    pub fn record_assignment(
        &mut self,
        span: swc_common::Span,
        scope: ScopeId,
        name: &str,
        provenance: BindingProvenance,
    ) {
        if !self.path_state.reachable {
            return;
        }
        self.budget.try_charge();
        let Some(name_id) = self.lookup_or_intern_name(name) else {
            self.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        self.record_assignment_value(span, scope, name_id, provenance);
    }

    fn record_assignment_value(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: glass_lint_datastructures::NameId,
        provenance: BindingProvenance,
    ) {
        let next = self
            .version_counters
            .entry(ScopedName::new(scope, name))
            .or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion::new(*next);
        self.path_state
            .assignment_environment
            .record_known(scope, name, provenance.clone());
        self.path_state
            .assignment_writes
            .insert(ScopedName::new(scope, name));
        self.assignments
            .push(crate::analysis::scope::AliasAssignment::single(
                span, scope, name, version, provenance,
            ));
    }

    fn record_join_assignment(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: glass_lint_datastructures::NameId,
        value: &ProvenanceAlternatives,
    ) {
        let next = self
            .version_counters
            .entry(ScopedName::new(scope, name))
            .or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion::new(*next);
        if value.has_complete_witness() {
            self.path_state
                .assignment_environment
                .record_alternatives(scope, name, value.clone());
        } else {
            self.path_state
                .assignment_environment
                .record_unknown(scope, name);
        }
        self.path_state
            .assignment_writes
            .insert(ScopedName::new(scope, name));
        self.assignments
            .push(crate::analysis::scope::AliasAssignment::joined(
                span,
                scope,
                name,
                version,
                value.clone(),
            ));
    }

    pub(super) fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        let name_id = self.name_id(name)?;
        for scope in self.stack.iter().rev().copied().map(ScopeId::new) {
            if let Some(assignment) = self
                .path_state
                .assignment_environment
                .get_by_id(scope, name_id)
            {
                return assignment
                    .preferred_witness()
                    .or(Some(&self.path_state.unknown_provenance));
            }
            if let Some(binding) = self.scopes[scope.index()].binding(name_id) {
                return Some(binding);
            }
        }
        None
    }

    pub(super) fn visible_binding_scope(&self, name: &str) -> Option<ScopeId> {
        let name_id = self.name_id(name)?;
        self.stack
            .iter()
            .rev()
            .copied()
            .map(ScopeId::new)
            .find(|scope| {
                self.path_state
                    .assignment_environment
                    .contains_by_id(*scope, name_id)
                    || self.scopes[scope.index()].has_binding(name_id)
            })
    }

    pub(super) fn is_unbound(&self, name: &str) -> bool {
        self.artifacts.scope_issues.is_empty() && self.visible_binding(name).is_none()
    }

    /// Record a checkpoint for later restore or join. This is cheap
    /// (does not clone the environment) because the environment uses a
    /// mutation log internally.
    fn checkpoint(&self) -> CollectorCheckpoint {
        CollectorCheckpoint {
            cursor: self.path_state.assignment_environment.checkpoint(),
            writes: self.path_state.assignment_writes.checkpoint(),
            reachable: self.path_state.reachable,
        }
    }

    /// Restore the assignment environment to a previously recorded
    /// checkpoint. O(delta) — only the entries changed since the
    /// checkpoint are rolled back.
    fn restore(&mut self, checkpoint: &CollectorCheckpoint) {
        self.path_state
            .assignment_environment
            .restore(checkpoint.cursor);
        self.path_state.assignment_writes.restore(checkpoint.writes);
        self.path_state.reachable = checkpoint.reachable;
    }

    /// Join multiple path environments at a control-flow merge point.
    ///
    /// Only keys written by a branch are read from each checkpoint. The
    /// live assignment table is transitioned between parent-linked cursors;
    /// no complete environment snapshot is allocated for a branch.
    fn join_paths(
        &mut self,
        span: Span,
        incoming: &CollectorCheckpoint,
        paths: &[CollectorCheckpoint],
    ) {
        let reachable: Vec<&CollectorCheckpoint> = paths.iter().filter(|p| p.reachable).collect();

        if reachable.is_empty() {
            self.restore(incoming);
            self.path_state.reachable = false;
            return;
        }

        // Collect all names written in any path. Record a join assignment
        // for every name touched by any path because the join may produce
        // provenance alternatives even when the incoming also wrote to the
        // same name (which the previous incoming-exclusion approach missed).
        let mut touched = BTreeSet::new();
        for path in paths {
            self.path_state.assignment_writes.restore(path.writes);
            touched.extend(self.path_state.assignment_writes.iter());
        }

        self.restore(incoming);
        for key in touched {
            let incoming_value = self
                .path_state
                .assignment_environment
                .get_by_id(key.scope(), key.name())
                .cloned()
                .unwrap_or_else(|| {
                    self.scopes[key.scope().index()]
                        .binding(key.name())
                        .cloned()
                        .map_or_else(
                            ProvenanceAlternatives::unknown,
                            ProvenanceAlternatives::single,
                        )
                });

            let mut value = ProvenanceAlternatives::joined();
            for path in &reachable {
                self.path_state.assignment_environment.restore(path.cursor);
                let path_value = self
                    .path_state
                    .assignment_environment
                    .get_by_id(key.scope(), key.name())
                    .cloned()
                    .unwrap_or_else(|| incoming_value.clone());
                value.add_bounded(&path_value, self.path_state.alternative_limit);
            }
            self.restore(incoming);
            self.record_join_assignment(
                Span::new(span.hi, span.hi),
                key.scope(),
                key.name(),
                &value,
            );
        }
    }

    pub(super) fn enter_if(&mut self) {
        let incoming = self.checkpoint();
        self.path_state.control_flow.push(ControlFlowFrame::If {
            incoming,
            consequent: None,
        });
        self.path_state.assignment_writes.clear();
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_add(1);
    }

    pub(super) fn enter_else(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::If {
                incoming,
                consequent,
            }) = self.path_state.control_flow.last_mut()
            else {
                return;
            };
            *consequent = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.path_state.assignment_writes.clear();
    }

    pub(super) fn exit_if(&mut self, span: Span, has_else: bool) {
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::If {
            incoming,
            consequent,
        }) = self.path_state.control_flow.pop()
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

    pub(super) fn enter_loop(&mut self, guaranteed: bool) {
        let incoming = self.checkpoint();
        self.path_state.control_flow.push(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        self.path_state.assignment_writes.clear();
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_add(1);
    }

    pub(super) fn exit_loop(&mut self, span: Span) {
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks,
            continues,
        }) = self.path_state.control_flow.pop()
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

    pub(super) fn enter_switch(&mut self) {
        let incoming = self.checkpoint();
        self.path_state.control_flow.push(ControlFlowFrame::Switch {
            incoming,
            cases: Vec::new(),
            breaks: Vec::new(),
        });
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_add(1);
    }

    pub(super) fn enter_switch_case(&mut self) {
        let incoming = {
            let Some(ControlFlowFrame::Switch { incoming, .. }) =
                self.path_state.control_flow.last()
            else {
                return;
            };
            incoming.clone()
        };
        self.restore(&incoming);
        self.path_state.assignment_writes.clear();
    }

    pub(super) fn exit_switch_case(&mut self) {
        let case = self.checkpoint();
        if let Some(ControlFlowFrame::Switch { cases, .. }) =
            self.path_state.control_flow.last_mut()
        {
            cases.push(case);
        }
    }

    pub(super) fn exit_switch(&mut self, span: Span) {
        self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Switch {
            incoming,
            cases,
            breaks,
        }) = self.path_state.control_flow.pop()
        else {
            return;
        };
        let mut paths = Vec::with_capacity(cases.len() + breaks.len() + 1);
        paths.push(incoming.clone());
        paths.extend(cases);
        paths.extend(breaks);
        self.join_paths(span, &incoming, &paths);
    }

    pub(super) fn enter_try(&mut self, has_handler: bool, has_finally: bool) {
        let incoming = self.checkpoint();
        self.path_state.control_flow.push(ControlFlowFrame::Try {
            incoming,
            body: None,
            conditional: has_handler || has_finally,
        });
        self.path_state.assignment_writes.clear();
        if has_handler || has_finally {
            self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_add(1);
        }
    }

    pub(super) fn enter_catch(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::Try { incoming, body, .. }) =
                self.path_state.control_flow.last_mut()
            else {
                return;
            };
            *body = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.path_state.assignment_writes.clear();
    }

    pub(super) fn exit_try(&mut self, span: Span, has_handler: bool, has_finally: bool) {
        let Some(ControlFlowFrame::Try {
            incoming,
            body,
            conditional,
        }) = self.path_state.control_flow.pop()
        else {
            return;
        };
        if conditional {
            self.path_state.conditional_depth = self.path_state.conditional_depth.saturating_sub(1);
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

    pub(super) fn mark_unreachable(&mut self) {
        self.path_state.reachable = false;
    }

    pub(super) fn break_exit(&mut self) {
        if self.path_state.reachable {
            let checkpoint = self.checkpoint();
            if let Some(frame) = self.path_state.control_flow.iter_mut().rev().find(|frame| {
                matches!(
                    frame,
                    ControlFlowFrame::Loop { .. } | ControlFlowFrame::Switch { .. }
                )
            }) {
                match frame {
                    ControlFlowFrame::Loop { breaks, .. }
                    | ControlFlowFrame::Switch { breaks, .. } => breaks.push(checkpoint),
                    _ => unreachable!("breakable frame was checked above"),
                }
            }
        }
        self.path_state.reachable = false;
    }

    pub(super) fn continue_exit(&mut self) {
        if self.path_state.reachable {
            let checkpoint = self.checkpoint();
            if let Some(ControlFlowFrame::Loop { continues, .. }) = self
                .path_state
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| matches!(frame, ControlFlowFrame::Loop { .. }))
            {
                continues.push(checkpoint);
            }
        }
        self.path_state.reachable = false;
    }

    pub(super) fn enter_function(&mut self) {
        let checkpoint = self.checkpoint();
        let control_depth = self.path_state.control_flow.len();
        self.path_state
            .function_checkpoints
            .push(super::FunctionCheckpoint {
                checkpoint,
                conditional_depth: self.path_state.conditional_depth,
                control_depth,
            });
        self.path_state.reachable = true;
        self.path_state.assignment_writes.clear();
    }

    pub(super) fn exit_function(&mut self) {
        let Some(super::FunctionCheckpoint {
            checkpoint,
            conditional_depth,
            control_depth,
        }) = self.path_state.function_checkpoints.pop()
        else {
            return;
        };
        self.path_state.control_flow.truncate(control_depth);
        self.path_state.conditional_depth = conditional_depth;
        self.restore(&checkpoint);
    }

    pub(super) fn rooted_expr_name(
        &self,
        expr: &Expr,
    ) -> Option<glass_lint_datastructures::SymbolPath> {
        rooted_expr_chain_with(self, expr)
    }

    pub(super) fn invalidate_member_root(
        &mut self,
        member: &swc_ecma_ast::MemberExpr,
        span: swc_common::Span,
    ) {
        let Some(root) = member_root_identifier(member) else {
            return;
        };
        if !matches!(
            self.visible_binding(root.sym.as_ref()),
            Some(
                BindingProvenance::StaticStringArray(_)
                    | BindingProvenance::StaticObjectKeys(_)
                    | BindingProvenance::StaticObjectValues(_)
            )
        ) {
            return;
        }
        let Some(root_id) = self.name_id(root.sym.as_ref()) else {
            return;
        };
        let Some(scope) = self
            .stack
            .iter()
            .rev()
            .find(|scope| self.scopes[**scope].has_binding(root_id))
        else {
            return;
        };
        self.record_assignment(
            span,
            ScopeId::new(*scope),
            root.sym.as_ref(),
            BindingProvenance::Local,
        );
    }
}
