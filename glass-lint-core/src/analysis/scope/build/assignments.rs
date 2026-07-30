use std::collections::BTreeSet;

use swc_common::Span;
use swc_ecma_ast::Expr;

use crate::analysis::{
    scope::{
        BindingProvenance, ScopeId, ScopedName,
        build::{
            CollectorCheckpoint, ControlFlowFrame, ScopeCollector,
            history::{AssignmentEnvironment, ProvenanceAlternatives},
        },
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
        if !self.reachable {
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
        let next = self.version_counters.entry((scope, name)).or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion(*next);
        self.assignment_environment
            .record_known(scope, name, provenance.clone());
        self.assignment_writes.insert(ScopedName::new(scope, name));
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
        let next = self.version_counters.entry((scope, name)).or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion(*next);
        if value.provenances.is_empty() {
            self.assignment_environment.record_unknown(scope, name);
        } else {
            self.assignment_environment
                .record_alternatives(scope, name, value.clone());
        }
        self.assignment_writes.insert(ScopedName::new(scope, name));
        self.assignments
            .push(crate::analysis::scope::AliasAssignment {
                span,
                scope,
                name,
                version,
                alternatives: value.provenances.clone(),
                unknown: value.unknown,
                joined: true,
            });
    }

    pub(super) fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        let name_id = self.name_id(name)?;
        for scope in self.stack.iter().rev().copied().map(ScopeId::from) {
            if let Some(assignment) = self.assignment_environment.get_by_id(scope, name_id) {
                if assignment.joined {
                    return assignment
                        .provenances
                        .iter()
                        .find(|p| !matches!(p, BindingProvenance::Local))
                        .or(Some(&self.unknown_provenance));
                }
                return assignment
                    .provenances
                    .first()
                    .or(Some(&self.unknown_provenance));
            }
            if let Some(binding) = self.scopes[scope.index()].bindings.get(&name_id) {
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
            .map(ScopeId::from)
            .find(|scope| {
                self.assignment_environment.contains_by_id(*scope, name_id)
                    || self.scopes[scope.index()].bindings.contains_key(&name_id)
            })
    }

    pub(super) fn is_unbound(&self, name: &str) -> bool {
        self.scope_issues.is_empty() && self.visible_binding(name).is_none()
    }

    /// Record a checkpoint for later restore or join. This is cheap
    /// (does not clone the environment) because the environment uses a
    /// mutation log internally.
    fn checkpoint(&self) -> CollectorCheckpoint {
        CollectorCheckpoint {
            cursor: self.assignment_environment.checkpoint(),
            writes: self.assignment_writes.clone(),
            reachable: self.reachable,
        }
    }

    /// Restore the assignment environment to a previously recorded
    /// checkpoint. O(delta) — only the entries changed since the
    /// checkpoint are rolled back.
    fn restore(&mut self, checkpoint: &CollectorCheckpoint) {
        self.assignment_environment.restore(checkpoint.cursor);
        self.assignment_writes.clone_from(&checkpoint.writes);
        self.reachable = checkpoint.reachable;
    }

    /// Join multiple path environments at a control-flow merge point.
    ///
    /// Temporarily transitions through each path's cursor position in
    /// the mutation log to snapshot each branch's state, then builds the
    /// joined environment and records synthetic join assignments for
    /// every name touched in any path.
    ///
    /// The incoming checkpoint is the state before the branching
    /// construct. Its cursor is used to obtain the incoming snapshot and
    /// as the fallback writes set.
    fn join_paths(
        &mut self,
        span: Span,
        incoming: &CollectorCheckpoint,
        paths: &[CollectorCheckpoint],
    ) {
        // Collect snapshots for each reachable path by transitioning to
        // that path's log cursor and cloning the live map.
        let mut path_snaps: Vec<(AssignmentEnvironment, &BTreeSet<ScopedName>)> = Vec::new();
        let reachable: Vec<&CollectorCheckpoint> = paths.iter().filter(|p| p.reachable).collect();
        for path in &reachable {
            self.assignment_environment.restore(path.cursor);
            path_snaps.push((self.assignment_environment.snapshot(), &path.writes));
        }

        // Snapshot the incoming environment
        self.assignment_environment.restore(incoming.cursor);
        let incoming_snapshot = self.assignment_environment.snapshot();

        if path_snaps.is_empty() {
            self.assignment_writes.clone_from(&incoming.writes);
            self.reachable = false;
            return;
        }

        // Build the list of all reachable path environments for the join
        let all_envs: Vec<&AssignmentEnvironment> =
            path_snaps.iter().map(|(snap, _)| snap).collect();

        // Compute the joined values separately, but keep the live environment
        // and its mutation log. Outer control-flow frames may still hold
        // checkpoints into that log; replacing it would make those cursors
        // point past the new log and cause a later restore to panic.
        let joined_environment = AssignmentEnvironment::join(&all_envs, self.alternative_limit);
        self.reachable = true;

        // Collect all names written in any path. Record a join assignment
        // for every name touched by any path because the join may produce
        // provenance alternatives even when the incoming also wrote to the
        // same name (which the previous incoming-exclusion approach missed).
        let mut touched = BTreeSet::new();
        for path in paths {
            touched.extend(path.writes.iter().cloned());
        }
        self.assignment_writes.clone_from(&incoming.writes);
        for key in touched {
            let mut value = joined_environment
                .get_by_id(key.scope(), key.name())
                .cloned()
                .unwrap_or_else(|| {
                    self.scopes[key.scope().index()]
                        .bindings
                        .get(&key.name())
                        .cloned()
                        .map_or_else(
                            ProvenanceAlternatives::unknown,
                            ProvenanceAlternatives::single,
                        )
                });

            // A missing branch entry means that branch retains the incoming
            // value. The environment only stores writes, so materialize that
            // value explicitly before recording the synthetic join. Without
            // this, `host` followed by a conditional `local` write loses the
            // host witness entirely.
            let any_missing = all_envs
                .iter()
                .any(|e| e.get_by_id(key.scope(), key.name()).is_none());
            if any_missing {
                let incoming_value = incoming_snapshot
                    .get_by_id(key.scope(), key.name())
                    .cloned()
                    .or_else(|| {
                        self.scopes[key.scope().index()]
                            .bindings
                            .get(&key.name())
                            .cloned()
                            .map(ProvenanceAlternatives::single)
                    })
                    .unwrap_or_else(ProvenanceAlternatives::unknown);
                value.add_bounded(&incoming_value, self.alternative_limit);
            }
            value = value.join_value();
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
        self.control_flow.push(ControlFlowFrame::If {
            incoming,
            consequent: None,
        });
        self.assignment_writes.clear();
        self.conditional_depth = self.conditional_depth.saturating_add(1);
    }

    pub(super) fn enter_else(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::If {
                incoming,
                consequent,
            }) = self.control_flow.last_mut()
            else {
                return;
            };
            *consequent = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment_writes.clear();
    }

    pub(super) fn exit_if(&mut self, span: Span, has_else: bool) {
        self.conditional_depth = self.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::If {
            incoming,
            consequent,
        }) = self.control_flow.pop()
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
        self.control_flow.push(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        self.assignment_writes.clear();
        self.conditional_depth = self.conditional_depth.saturating_add(1);
    }

    pub(super) fn exit_loop(&mut self, span: Span) {
        self.conditional_depth = self.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks,
            continues,
        }) = self.control_flow.pop()
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
        self.control_flow.push(ControlFlowFrame::Switch {
            incoming,
            cases: Vec::new(),
            breaks: Vec::new(),
        });
        self.conditional_depth = self.conditional_depth.saturating_add(1);
    }

    pub(super) fn enter_switch_case(&mut self) {
        let incoming = {
            let Some(ControlFlowFrame::Switch { incoming, .. }) = self.control_flow.last() else {
                return;
            };
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment_writes.clear();
    }

    pub(super) fn exit_switch_case(&mut self) {
        let case = self.checkpoint();
        if let Some(ControlFlowFrame::Switch { cases, .. }) = self.control_flow.last_mut() {
            cases.push(case);
        }
    }

    pub(super) fn exit_switch(&mut self, span: Span) {
        self.conditional_depth = self.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Switch {
            incoming,
            cases,
            breaks,
        }) = self.control_flow.pop()
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
        self.control_flow.push(ControlFlowFrame::Try {
            incoming,
            body: None,
            conditional: has_handler || has_finally,
        });
        self.assignment_writes.clear();
        if has_handler || has_finally {
            self.conditional_depth = self.conditional_depth.saturating_add(1);
        }
    }

    pub(super) fn enter_catch(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::Try { incoming, body, .. }) = self.control_flow.last_mut()
            else {
                return;
            };
            *body = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment_writes.clear();
    }

    pub(super) fn exit_try(&mut self, span: Span, has_handler: bool, has_finally: bool) {
        let Some(ControlFlowFrame::Try {
            incoming,
            body,
            conditional,
        }) = self.control_flow.pop()
        else {
            return;
        };
        if conditional {
            self.conditional_depth = self.conditional_depth.saturating_sub(1);
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
        self.reachable = false;
    }

    pub(super) fn break_exit(&mut self) {
        if self.reachable {
            let checkpoint = self.checkpoint();
            if let Some(frame) = self.control_flow.iter_mut().rev().find(|frame| {
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
        self.reachable = false;
    }

    pub(super) fn continue_exit(&mut self) {
        if self.reachable {
            let checkpoint = self.checkpoint();
            if let Some(ControlFlowFrame::Loop { continues, .. }) = self
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| matches!(frame, ControlFlowFrame::Loop { .. }))
            {
                continues.push(checkpoint);
            }
        }
        self.reachable = false;
    }

    pub(super) fn enter_function(&mut self) {
        let checkpoint = self.checkpoint();
        let control_depth = self.control_flow.len();
        self.function_checkpoints
            .push((checkpoint, self.conditional_depth, control_depth));
        self.reachable = true;
        self.assignment_writes.clear();
    }

    pub(super) fn exit_function(&mut self) {
        let Some((checkpoint, conditional_depth, control_depth)) = self.function_checkpoints.pop()
        else {
            return;
        };
        self.control_flow.truncate(control_depth);
        self.conditional_depth = conditional_depth;
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
            .find(|scope| self.scopes[**scope].bindings.contains_key(&root_id))
        else {
            return;
        };
        self.record_assignment(
            span,
            ScopeId::from(*scope),
            root.sym.as_ref(),
            BindingProvenance::Local,
        );
    }
}
