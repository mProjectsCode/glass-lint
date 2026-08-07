use std::collections::BTreeSet;

use swc_common::Span;
use swc_ecma_ast::Expr;

use crate::analysis::{
    model::scope::BindingVersion,
    scope::{
        BindingProvenance, ProvenanceAlternatives, ScopeId, ScopedName,
        build::{CollectorCheckpoint, ControlFlowFrame, ScopeCollectionIssue, ScopeCollector},
        query::rooted::rooted_expr_chain_with,
    },
    syntax::member_root_identifier,
};

type JoinedPathAssignments = Vec<(
    ScopeId,
    glass_lint_datastructures::NameId,
    ProvenanceAlternatives,
)>;

impl super::PathCollectionState {
    fn join_paths(
        &mut self,
        incoming: &CollectorCheckpoint,
        paths: &[CollectorCheckpoint],
        fallback: impl Fn(ScopeId, glass_lint_datastructures::NameId) -> ProvenanceAlternatives,
    ) -> Result<Option<JoinedPathAssignments>, super::history::HistoryRestoreError> {
        let reachable: Vec<&CollectorCheckpoint> =
            paths.iter().filter(|path| path.reachable).collect();

        if reachable.is_empty() {
            self.restore_checkpoint(incoming)?;
            self.reachable = false;
            return Ok(None);
        }

        let mut touched = BTreeSet::new();
        for path in paths {
            self.assignment_writes.restore(path.writes)?;
            touched.extend(self.assignment_writes.iter());
        }
        self.restore_checkpoint(incoming)?;

        let mut joined = Vec::with_capacity(touched.len());
        for key in touched {
            let incoming_value = self
                .assignment_environment
                .get_by_id(key.scope(), key.name())
                .cloned()
                .unwrap_or_else(|| fallback(key.scope(), key.name()));
            let mut value = ProvenanceAlternatives::joined();
            for path in &reachable {
                self.assignment_environment.restore(path.cursor)?;
                let path_value = self
                    .assignment_environment
                    .get_by_id(key.scope(), key.name())
                    .cloned()
                    .unwrap_or_else(|| incoming_value.clone());
                value.add_bounded(&path_value, self.alternative_limit);
            }
            self.restore_checkpoint(incoming)?;
            joined.push((key.scope(), key.name(), value));
        }
        Ok(Some(joined))
    }

    fn restore_checkpoint(
        &mut self,
        checkpoint: &CollectorCheckpoint,
    ) -> Result<(), super::history::HistoryRestoreError> {
        if let Err(error) = self.assignment_environment.restore(checkpoint.cursor) {
            self.reachable = false;
            return Err(error);
        }
        if let Err(error) = self.assignment_writes.restore(checkpoint.writes) {
            self.reachable = false;
            return Err(error);
        }
        Ok(())
    }
}

impl ScopeCollector<'_> {
    pub fn record_assignment(
        &mut self,
        span: swc_common::Span,
        scope: ScopeId,
        name: &str,
        provenance: BindingProvenance,
    ) {
        if !self.assignment.path.reachable {
            return;
        }
        self.budget.try_charge();
        let Some(name_id) = self.lookup_or_intern_name(name) else {
            self.lexical.name_exhausted = true;
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
        let version = self.next_assignment_version(scope, name);
        self.assignment
            .path
            .assignment_environment
            .record_known(scope, name, provenance.clone());
        self.assignment
            .assignments
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
        let version = self.next_assignment_version(scope, name);
        if value.has_complete_witness() {
            self.assignment
                .path
                .assignment_environment
                .record_alternatives(scope, name, value.clone());
        } else {
            self.assignment
                .path
                .assignment_environment
                .record_unknown(scope, name);
        }
        self.assignment
            .assignments
            .push(crate::analysis::scope::AliasAssignment::joined(
                span,
                scope,
                name,
                version,
                value.clone(),
            ));
    }

    fn next_assignment_version(
        &mut self,
        scope: ScopeId,
        name: glass_lint_datastructures::NameId,
    ) -> BindingVersion {
        let key = ScopedName::new(scope, name);
        let next = self
            .assignment
            .version_counters
            .entry(key.clone())
            .or_insert(0);
        *next = next.saturating_add(1);
        self.assignment.path.assignment_writes.insert(key);
        BindingVersion::new(*next)
    }

    fn visible_binding_with_scope(&self, name: &str) -> Option<(ScopeId, &BindingProvenance)> {
        let name_id = self.name_id(name)?;
        for scope in self.lexical.stack.iter().rev().copied() {
            if let Some(assignment) = self
                .assignment
                .path
                .assignment_environment
                .get_by_id(scope, name_id)
            {
                let provenance = assignment
                    .preferred_witness()
                    .unwrap_or(&self.assignment.path.unknown_provenance);
                return Some((scope, provenance));
            }
            if let Some(binding) = self
                .lexical
                .scopes
                .get(scope)
                .and_then(|scope| scope.binding(name_id))
            {
                return Some((scope, binding));
            }
        }
        None
    }

    pub(super) fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        self.visible_binding_with_scope(name)
            .map(|(_, provenance)| provenance)
    }

    pub(super) fn visible_binding_scope(&self, name: &str) -> Option<ScopeId> {
        self.visible_binding_with_scope(name)
            .map(|(scope, _)| scope)
    }

    pub(super) fn is_unbound(&self, name: &str) -> bool {
        !self.artifacts.has_issues() && self.visible_binding(name).is_none()
    }

    /// Record a checkpoint for later restore or join. This is cheap
    /// (does not clone the environment) because the environment uses a
    /// mutation log internally.
    fn checkpoint(&self) -> CollectorCheckpoint {
        CollectorCheckpoint {
            cursor: self.assignment.path.assignment_environment.checkpoint(),
            writes: self.assignment.path.assignment_writes.checkpoint(),
            reachable: self.assignment.path.reachable,
        }
    }

    /// Restore the assignment environment to a previously recorded
    /// checkpoint. O(delta) — only the entries changed since the
    /// checkpoint are rolled back.
    fn restore(&mut self, checkpoint: &CollectorCheckpoint) -> bool {
        let assignment_result = self
            .assignment
            .path
            .assignment_environment
            .restore(checkpoint.cursor);
        let writes_result = self
            .assignment
            .path
            .assignment_writes
            .restore(checkpoint.writes);
        if assignment_result.is_err() || writes_result.is_err() {
            self.record_checkpoint_failure();
            return false;
        }
        self.assignment.path.reachable = checkpoint.reachable;
        true
    }

    fn record_checkpoint_failure(&mut self) {
        self.artifacts
            .record_issue(ScopeCollectionIssue::InvalidCheckpoint);
        self.assignment.path.reachable = false;
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
        let Ok(joined) = self
            .assignment
            .path
            .join_paths(incoming, paths, |scope, name| {
                self.lexical
                    .scopes
                    .get(scope)
                    .and_then(|scope| scope.binding(name))
                    .cloned()
                    .map_or_else(
                        ProvenanceAlternatives::unknown,
                        ProvenanceAlternatives::single,
                    )
            })
        else {
            self.record_checkpoint_failure();
            return;
        };
        let Some(joined) = joined else {
            return;
        };

        for (scope, name, value) in joined {
            self.record_join_assignment(Span::new(span.hi, span.hi), scope, name, &value);
        }
    }

    pub(super) fn enter_if(&mut self) {
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

    pub(super) fn enter_else(&mut self) {
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

    pub(super) fn exit_if(&mut self, span: Span, has_else: bool) {
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

    pub(super) fn enter_loop(&mut self, guaranteed: bool) {
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

    pub(super) fn exit_loop(&mut self, span: Span) {
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

    pub(super) fn enter_switch(&mut self) {
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

    pub(super) fn enter_switch_case(&mut self) {
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

    pub(super) fn exit_switch_case(&mut self) {
        let case = self.checkpoint();
        if let Some(ControlFlowFrame::Switch { cases, .. }) =
            self.assignment.path.control_flow.last_mut()
        {
            cases.push(case);
        }
    }

    pub(super) fn exit_switch(&mut self, span: Span) {
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

    pub(super) fn enter_try(&mut self, has_handler: bool, has_finally: bool) {
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

    pub(super) fn enter_catch(&mut self) {
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

    pub(super) fn exit_try(&mut self, span: Span, has_handler: bool, has_finally: bool) {
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

    pub(super) fn mark_unreachable(&mut self) {
        self.assignment.path.reachable = false;
    }

    pub(super) fn break_exit(&mut self) {
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

    pub(super) fn continue_exit(&mut self) {
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

    pub(super) fn enter_function(&mut self) {
        let checkpoint = self.checkpoint();
        let control_depth = self.assignment.path.control_flow.len();
        self.assignment
            .path
            .function_checkpoints
            .push(super::FunctionCheckpoint {
                checkpoint,
                conditional_depth: self.assignment.path.conditional_depth,
                control_depth,
            });
        self.assignment.path.reachable = true;
        self.assignment.path.assignment_writes.clear();
    }

    pub(super) fn exit_function(&mut self) {
        let Some(super::FunctionCheckpoint {
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
        let Some(scope) = self.lexical.stack.iter().rev().find(|scope| {
            self.lexical
                .scopes
                .get(**scope)
                .is_some_and(|scope| scope.has_binding(root_id))
        }) else {
            return;
        };
        self.record_assignment(span, *scope, root.sym.as_ref(), BindingProvenance::Local);
    }
}
