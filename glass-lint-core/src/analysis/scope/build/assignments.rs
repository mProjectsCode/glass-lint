use swc_ecma_ast::Expr;

use crate::analysis::{
    scope::{
        BindingProvenance, ScopeId, build::ScopeCollector, query::rooted::rooted_expr_chain_with,
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
        self.budget.try_charge();
        let Some(name_id) = self.lookup_or_intern_name(name) else {
            self.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        let next = self.version_counters.entry((scope, name_id)).or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion(*next);
        self.latest_assignments
            .record(&self.names, scope, name, provenance.clone());
        self.assignments
            .push(crate::analysis::scope::AliasAssignment {
                span,
                scope,
                name: name_id,
                version,
                provenance,
                conditional: self.conditional_depth > 0,
            });
    }

    pub(super) fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        for scope in self.stack.iter().rev().copied().map(ScopeId::from) {
            if let Some(assignment) = self.latest_assignments.get(&self.names, scope, name) {
                return Some(assignment);
            }
            if let Some(binding) = self
                .name_id(name)
                .and_then(|name| self.scopes[scope.index()].bindings.get(&name))
            {
                return Some(binding);
            }
        }
        None
    }

    pub(super) fn visible_binding_scope(&self, name: &str) -> Option<ScopeId> {
        self.stack
            .iter()
            .rev()
            .copied()
            .map(ScopeId::from)
            .find(|scope| {
                self.latest_assignments.contains(&self.names, *scope, name)
                    || self
                        .name_id(name)
                        .is_some_and(|name| self.scopes[scope.index()].bindings.contains_key(&name))
            })
    }

    pub(super) fn is_unbound(&self, name: &str) -> bool {
        self.scope_issues.is_empty() && self.visible_binding(name).is_none()
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
        let Some(scope) = self.stack.iter().rev().find(|scope| {
            self.name_id(root.sym.as_ref())
                .is_some_and(|name| self.scopes[**scope].bindings.contains_key(&name))
        }) else {
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
