//! Lexical binding, scope, assignment-version, and shadowing queries.

use crate::analysis::scope::{
    frozen_assignments::AssignmentAt,
    query::{
        BindingKey, BindingProvenance, BindingRoot, BindingVersion, BoundArgument, Expr,
        FrozenScopeGraph, Ident, ScopeId, ScopeKind, Span,
    },
};

impl FrozenScopeGraph {
    pub(in crate::analysis) fn constructed_instance_at(
        &self,
        ident: &Ident,
    ) -> Option<(smol_str::SmolStr, smol_str::SmolStr)> {
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ConstructedInstance { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    /// Resolve one strict binding provenance visible at a use position.
    ///
    /// For a synthetic join, returns the first non-local witness. Callers
    /// whose identity check can distinguish alternatives should use
    /// `binding_alternatives_at` instead.
    pub(in crate::analysis) fn binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        match self.assignment_at(scope, self.name_id(name)?, span) {
            AssignmentAt::Known(assignment) => assignment.alternatives.first(),
            AssignmentAt::Ambiguous(assignment) => assignment
                .alternatives
                .iter()
                .find(|p| !matches!(p, BindingProvenance::Local)),
            AssignmentAt::Absent => self
                .function_for_scope(scope)
                .and_then(|function| {
                    self.name_id(name)
                        .and_then(|name| self.parameter_alias_for(function, name))
                })
                .or(Some(declaration)),
        }
    }

    /// Return all strict provenance alternatives visible at a use position.
    ///
    /// A synthetic join can also retain the declaration as the value on a
    /// path that did not write the binding. Unknown alternatives are not
    /// returned: callers may use the returned values as complete witnesses,
    /// while the presence of an unknown alternative remains available on the
    /// assignment record for certainty accounting.
    pub(in crate::analysis) fn binding_alternatives_at(
        &self,
        name: &str,
        span: Span,
    ) -> Vec<&BindingProvenance> {
        let Some((scope, declaration)) = self.binding_with_scope_at(name, span) else {
            return Vec::new();
        };
        let Some(name_id) = self.name_id(name) else {
            return Vec::new();
        };
        match self.assignment_at(scope, name_id, span) {
            AssignmentAt::Known(assignment) => {
                if assignment.unknown && assignment.alternatives.is_empty() {
                    Vec::new()
                } else {
                    assignment.alternatives.iter().collect()
                }
            }
            AssignmentAt::Ambiguous(assignment) => {
                if assignment.unknown && assignment.alternatives.is_empty() {
                    return Vec::new();
                }
                assignment.alternatives.iter().collect()
            }
            AssignmentAt::Absent => self
                .function_for_scope(scope)
                .and_then(|function| self.parameter_alias_for(function, name_id))
                .map_or_else(|| vec![declaration], |parameter| vec![parameter]),
        }
    }

    /// Resolve an expression to a stable lexical identity.  Semantic clients
    /// use this instead of rebuilding identity from the expression's printed
    /// member chain.
    pub(in crate::analysis) fn binding_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => {
                let (scope, _) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
                let binding = self.binding_id_at(scope, self.name_id(ident.sym.as_ref())?)?;
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
        let Some(name) = self.name_id(name) else {
            return BindingVersion::new(0);
        };
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
                binding: self.binding_id_at(scope, self.name_id(name)?)?,
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
        let name_id = self.name_id(name)?;
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scope_binding(scope, name_id) {
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
