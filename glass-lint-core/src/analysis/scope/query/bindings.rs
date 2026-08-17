//! Lexical binding, scope, assignment-version, and shadowing queries.

use glass_lint_datastructures::NameId;

use crate::analysis::scope::{
    frozen_assignments::{BindingResolution, BindingResolutionStatus},
    query::{
        BindingKey, BindingProvenance, Expr, FrozenScopeGraph, Ident, ScopeId, ScopeKind, Span,
    },
};

#[derive(Clone, Copy)]
enum RootMode {
    Lexical,
    Global,
    LexicalOrGlobal,
}

impl FrozenScopeGraph {
    pub(in crate::analysis) fn constructed_instance_at(
        &self,
        ident: &Ident,
    ) -> Option<(smol_str::SmolStr, smol_str::SmolStr)> {
        match self.definite_binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ConstructedInstance { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    /// Resolve a binding only when its provenance is complete at the use
    /// position. Joined or incomplete alternatives cannot establish a
    /// definite positive classification.
    pub(in crate::analysis) fn definite_binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        let resolution = self.binding_resolution_at(name, span);
        (resolution.status() == BindingResolutionStatus::Complete)
            .then(|| resolution.preferred_witness())
            .flatten()
    }

    /// Resolve a binding while retaining completeness and fallback status.
    pub(in crate::analysis) fn binding_resolution_at(
        &self,
        name: &str,
        span: Span,
    ) -> BindingResolution<'_> {
        let Some(name) = self.name_id(name) else {
            return BindingResolution::absent();
        };
        let Some(use_scope) = self.scope_at(span) else {
            return BindingResolution::absent();
        };
        self.resolve_binding(name, use_scope, span)
            .map_or_else(BindingResolution::absent, |(_, resolution)| resolution)
    }

    /// Resolve one binding from an already-located use scope, returning the
    /// binding's owning scope together with its resolution.
    ///
    /// Shared by [`Self::binding_resolution_at`] and the provenance-seed
    /// path so the scope walk, parameter lookup, and assignment resolution
    /// cannot diverge between them.
    pub(super) fn resolve_binding(
        &self,
        name: NameId,
        use_scope: ScopeId,
        span: Span,
    ) -> Option<(ScopeId, BindingResolution<'_>)> {
        let (scope, declaration) = self.nearest_binding_from_scope(name, use_scope)?;
        let parameter = self.parameter_alias_for_scope(scope, name);
        let resolution = self
            .assignment_at(scope, name, span)
            .resolve(parameter, declaration);
        Some((scope, resolution))
    }

    /// Resolve an expression to a stable lexical identity.  Semantic clients
    /// use this instead of rebuilding identity from the expression's printed
    /// member chain.
    pub(in crate::analysis) fn binding_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        self.expression_key(expr, RootMode::Lexical)
    }

    fn expression_key(&self, expr: &Expr, mode: RootMode) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => self.identifier_key(ident, mode),
            Expr::Member(member) => {
                let child_mode = match mode {
                    RootMode::Lexical => RootMode::LexicalOrGlobal,
                    mode => mode,
                };
                let mut key = self.expression_key(&member.obj, child_mode)?;
                key.append_segment(
                    self.name_id(self.contextual_member_property_name(member)?.as_str())?,
                );
                Some(key)
            }
            Expr::This(_) => Some(BindingKey::global("this")),
            Expr::Paren(paren) => self.expression_key(&paren.expr, mode),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.expression_key(expr, mode)),
            _ => None,
        }
    }

    fn identifier_key(&self, ident: &Ident, mode: RootMode) -> Option<BindingKey> {
        match mode {
            RootMode::Lexical => self.lexical_identifier_key(ident),
            RootMode::Global => (self
                .binding_resolution_at(ident.sym.as_ref(), ident.span)
                .status()
                == BindingResolutionStatus::Absent)
                .then(|| BindingKey::global(ident.sym.to_string())),
            RootMode::LexicalOrGlobal => self
                .lexical_identifier_key(ident)
                .or_else(|| self.identifier_key(ident, RootMode::Global)),
        }
    }

    fn lexical_identifier_key(&self, ident: &Ident) -> Option<BindingKey> {
        let (scope, _) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
        let name = self.name_id(ident.sym.as_ref())?;
        self.lexical_binding_key(scope, name, ident.span)
    }

    /// Find the nearest lexical declaration and its owning scope.
    pub(in crate::analysis) fn binding_with_scope_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        let name = self.name_id(name)?;
        self.nearest_binding_at(name, span)
    }

    /// Whether `with` or prior unshadowed `eval` invalidates lookup here.
    pub(in crate::analysis) fn has_dynamic_lookup_at(&self, span: Span) -> bool {
        let Some(scope) = self.scope_at(span) else {
            return true;
        };
        self.scope_or_ancestor_has_kind(scope, ScopeKind::Dynamic)
            || self.has_prior_eval(scope, span)
    }

    /// Test a scope and all parents for a specific scope kind.
    pub(in crate::analysis) fn scope_or_ancestor_has_kind(
        &self,
        scope: ScopeId,
        kind: ScopeKind,
    ) -> bool {
        self.scope_ancestors(scope)
            .any(|scope| self.scope_kind(scope) == Some(kind))
    }

    /// Require a configured global to be unshadowed and dynamically resolvable.
    pub(in crate::analysis) fn unshadowed_global_at(&self, name: &str, span: Span) -> bool {
        self.is_global(name) && self.unshadowed_unbound_at(name, span)
    }

    /// Require an identifier to have no lexical or dynamic binding.
    pub(in crate::analysis) fn unshadowed_unbound_at(&self, name: &str, span: Span) -> bool {
        !self.has_dynamic_lookup_at(span)
            && self.binding_resolution_at(name, span).status() == BindingResolutionStatus::Absent
    }
}
