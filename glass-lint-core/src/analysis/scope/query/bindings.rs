//! Lexical binding, scope, assignment-version, and shadowing queries.

use crate::analysis::scope::{
    frozen_assignments::{BindingResolution, BindingResolutionStatus},
    query::{
        BindingKey, BindingProvenance, BindingVersion, Expr, FrozenScopeGraph, Ident, ScopeId,
        ScopeKind, Span,
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
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ConstructedInstance { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    /// Resolve one strict binding provenance visible at a use position.
    ///
    /// This is a convenience projection that intentionally discards joined
    /// and incomplete status. Callers making fallback or certainty decisions
    /// must use [`Self::binding_resolution_at`] instead.
    pub(in crate::analysis) fn binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        self.binding_resolution_at(name, span).preferred_witness()
    }

    /// Resolve a binding while retaining completeness and fallback status.
    pub(in crate::analysis) fn binding_resolution_at(
        &self,
        name: &str,
        span: Span,
    ) -> BindingResolution<'_> {
        let Some((scope, declaration)) = self.binding_with_scope_at(name, span) else {
            return BindingResolution::absent();
        };
        let Some(name_id) = self.name_id(name) else {
            return BindingResolution::absent();
        };
        let parameter = self.parameter_alias_for_scope(scope, name_id);
        self.assignment_at(scope, name_id, span)
            .resolve(parameter, declaration)
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
        let binding = self.binding_id_at(scope, self.name_id(ident.sym.as_ref())?)?;
        Some(BindingKey::lexical(
            self.function_scope_at(scope),
            binding,
            self.binding_version_at(scope, ident.sym.as_ref(), ident.span),
        ))
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
            return Some(BindingKey::lexical(
                self.function_scope_at(scope),
                self.binding_id_at(scope, self.name_id(name)?)?,
                self.binding_version_at(scope, name, span),
            ));
        }
        Some(BindingKey::global(name))
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

    /// Require a configured global to be unshadowed and dynamically resolvable.
    pub(in crate::analysis) fn unshadowed_global_at(&self, name: &str, span: Span) -> bool {
        self.is_global(name)
            && !self.has_dynamic_lookup_at(span)
            && self.binding_resolution_at(name, span).status() == BindingResolutionStatus::Absent
    }

    /// Require an identifier to have no lexical or dynamic binding.
    pub(in crate::analysis) fn unshadowed_unbound_at(&self, name: &str, span: Span) -> bool {
        !self.has_dynamic_lookup_at(span)
            && self.binding_resolution_at(name, span).status() == BindingResolutionStatus::Absent
    }
}
