//! Lexical binding, scope, assignment-version, and shadowing queries.

use crate::analysis::scope::query::{
    BindingKey, BindingProvenance, BindingRoot, BindingVersion, BoundArgument, Expr, Ident,
    ScopeGraph, ScopeId, ScopeKind, Span,
};

impl ScopeGraph {
    /// Resolve the binding provenance visible at a use position.
    pub(in crate::analysis) fn binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        // A declaration is the fallback. The last assignment at or before the
        // read wins, which is why assignments are sorted once during collection
        // and selected with `partition_point` here.
        self.assignment_at(scope, name, span)
            .map(|assignment| &assignment.provenance)
            .or_else(|| self.parameter_alias_for(scope, name))
            .or(Some(declaration))
    }

    /// Resolve an expression to a stable lexical identity.  Semantic clients
    /// use this instead of rebuilding identity from the expression's printed
    /// member chain.
    pub(in crate::analysis) fn binding_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => {
                let (scope, _) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
                let binding = self.binding_id_at(scope, ident.sym.as_ref())?;
                Some(BindingKey::new(BindingRoot::Binding {
                    function: self.function_scope_at(scope),
                    binding,
                    version: self.binding_version_at(scope, ident.sym.as_ref(), ident.span),
                }))
            }
            Expr::Member(member) => {
                let mut key = self
                    .binding_key_for_expr(&member.obj)
                    .or_else(|| self.global_key_for_expr(&member.obj))?;
                key.append_segment(self.name_id(self.member_property_name(member)?.as_str())?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey::new(BindingRoot::Global("this".into()))),
            Expr::Paren(paren) => self.binding_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.binding_key_for_expr(expr)),
            _ => None,
        }
    }

    /// Derive a global-rooted key only when no lexical binding shadows it.
    pub(in crate::analysis) fn global_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => self
                .binding_at(ident.sym.as_ref(), ident.span)
                .is_none()
                .then(|| BindingKey::new(BindingRoot::Global(ident.sym.to_string()))),
            Expr::Member(member) => {
                let mut key = self.global_key_for_expr(&member.obj)?;
                key.append_segment(self.name_id(self.member_property_name(member)?.as_str())?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey::new(BindingRoot::Global("this".into()))),
            Expr::Paren(paren) => self.global_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.global_key_for_expr(expr)),
            _ => None,
        }
    }

    /// Return the assignment version visible at a source position.
    pub(in crate::analysis) fn binding_version_at(
        &self,
        scope: ScopeId,
        name: &str,
        span: Span,
    ) -> BindingVersion {
        self.binding_version(scope, name, span)
    }

    /// Build a stable key for a name, using a global root when unbound.
    pub(in crate::analysis) fn binding_key_for_name(
        &self,
        name: &str,
        span: Span,
    ) -> Option<BindingKey> {
        if let Some((scope, _)) = self.binding_with_scope_at(name, span) {
            return Some(BindingKey::new(BindingRoot::Binding {
                function: self.function_scope_at(scope),
                binding: self.binding_id_at(scope, name)?,
                version: self.binding_version_at(scope, name, span),
            }));
        }
        Some(BindingKey::new(BindingRoot::Global(name.to_string())))
    }

    /// Find the nearest lexical declaration and its owning scope.
    pub(in crate::analysis) fn binding_with_scope_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scope_binding(scope, name) {
                return Some((scope, binding));
            }
            scope = self.scope_parent(scope)?;
        }
    }

    /// Whether `with` or prior unshadowed `eval` invalidates lookup here.
    pub(in crate::analysis) fn has_dynamic_lookup_at(&self, span: Span) -> bool {
        let scope = self.scope_at(span);
        self.scope_or_ancestor_has_kind(scope, ScopeKind::Dynamic)
            || self.has_prior_eval(scope, span)
    }

    /// Test a scope and all parents for a specific scope kind.
    pub(in crate::analysis) fn scope_or_ancestor_has_kind(
        &self,
        mut scope: ScopeId,
        kind: ScopeKind,
    ) -> bool {
        loop {
            if self.scope_kind(scope) == Some(kind) {
                return true;
            }
            let Some(parent) = self.scope_parent(scope) else {
                return false;
            };
            scope = parent;
        }
    }

    /// Return static arguments captured by a supported bound callable.
    pub(in crate::analysis) fn bound_arguments(
        &self,
        ident: &Ident,
    ) -> Option<Vec<Option<BoundArgument>>> {
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::BoundCallable {
                bound_arguments, ..
            }
            | BindingProvenance::BoundModuleCallable {
                bound_arguments, ..
            } => Some(bound_arguments.clone()),
            _ => None,
        }
    }

    /// Require a configured global to be unshadowed and dynamically resolvable.
    pub(in crate::analysis) fn unshadowed_global_at(&self, name: &str, span: Span) -> bool {
        self.is_global(name)
            && !self.has_dynamic_lookup_at(span)
            && self.binding_at(name, span).is_none()
    }

    /// Require an identifier to have no lexical or dynamic binding.
    pub(in crate::analysis) fn unshadowed_unbound_at(&self, name: &str, span: Span) -> bool {
        !self.has_dynamic_lookup_at(span) && self.binding_at(name, span).is_none()
    }
}
