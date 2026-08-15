use std::collections::BTreeSet;

use swc_common::Span;
use swc_ecma_ast::Expr;

use crate::analysis::{
    model::scope::BindingVersion,
    scope::{
        AliasAssignment, BindingProvenance, ProvenanceAlternatives, ProvenanceJoin, ScopeId,
        ScopedName,
        build::{CollectorCheckpoint, ScopeCollectionIssue, ScopeCollector},
        query::rooted::rooted_expr_chain_with,
    },
    syntax::member_root_identifier,
};

type JoinedPathAssignments = Vec<(ScopeId, glass_lint_datastructures::NameId, ProvenanceJoin)>;

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
            let mut value = ProvenanceJoin::new(self.alternative_limit);
            for path in &reachable {
                self.assignment_environment.restore(path.cursor)?;
                let path_value = self
                    .assignment_environment
                    .get_by_id(key.scope(), key.name())
                    .cloned()
                    .unwrap_or_else(|| incoming_value.clone());
                value.add(&path_value);
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
    pub(super) fn record_assignment(
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
        self.push_assignment(
            span,
            scope,
            name,
            ProvenanceAlternatives::single(provenance),
        );
    }

    fn record_join_assignment(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: glass_lint_datastructures::NameId,
        value: &ProvenanceJoin,
    ) {
        self.push_assignment(span, scope, name, value.alternatives().clone());
    }

    /// Version, write, and push one assignment in source order.
    fn push_assignment(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: glass_lint_datastructures::NameId,
        alternatives: ProvenanceAlternatives,
    ) {
        let version = self.next_assignment_version(scope, name);
        if alternatives.has_complete_witness() {
            self.assignment
                .path
                .assignment_environment
                .record_alternatives(scope, name, alternatives.clone());
        } else {
            self.assignment
                .path
                .assignment_environment
                .record_unknown(scope, name);
        }
        self.assignment
            .assignments
            .push(AliasAssignment::from_alternatives(
                span,
                scope,
                name,
                version,
                alternatives,
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
                    .unwrap_or(&super::UNKNOWN_PROVENANCE);
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
    pub(super) fn checkpoint(&self) -> CollectorCheckpoint {
        CollectorCheckpoint {
            cursor: self.assignment.path.assignment_environment.checkpoint(),
            writes: self.assignment.path.assignment_writes.checkpoint(),
            reachable: self.assignment.path.reachable,
        }
    }

    /// Restore the assignment environment to a previously recorded
    /// checkpoint. O(delta) — only the entries changed since the
    /// checkpoint are rolled back.
    pub(super) fn restore(&mut self, checkpoint: &CollectorCheckpoint) -> bool {
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

    pub(super) fn record_checkpoint_failure(&mut self) {
        self.artifacts
            .record_issue(ScopeCollectionIssue::InvalidCheckpoint);
        self.assignment.path.reachable = false;
    }

    /// Join multiple path environments at a control-flow merge point.
    ///
    /// Only keys written by a branch are read from each checkpoint. The
    /// live assignment table is transitioned between parent-linked cursors;
    /// no complete environment snapshot is allocated for a branch.
    pub(super) fn join_paths(
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
