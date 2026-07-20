//! Function identity queries across lexical scopes and aliases.

use super::{BindingProvenance, Expr, FunctionId, ScopeGraph, ScopeId, Span};

impl ScopeGraph<'_> {
    /// Find the nearest enclosing function identity for a lexical scope.
    pub(in crate::analysis) fn function_scope_at(&self, scope: ScopeId) -> FunctionId {
        let mut current = Some(scope);
        while let Some(index) = current {
            if let Some(function) = self.function_for_scope(index) {
                return function;
            }
            current = self.scope_parent(index);
        }
        FunctionId(0)
    }

    /// Return the canonical function identity for a scope.
    pub(in crate::analysis) fn function_id_for_scope(&self, scope: ScopeId) -> FunctionId {
        self.function_scope_at(scope)
    }

    /// Resolve a function identifier/alias if it was not reassigned before use.
    pub(in crate::analysis) fn function_id_for_expr(&self, expr: &Expr) -> Option<FunctionId> {
        let Expr::Ident(ident) = expr else {
            return None;
        };
        let (scope, provenance) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
        let function =
            self.function_binding(scope, ident.sym.as_ref())
                .or_else(|| self.function_alias(scope, ident.sym.as_ref()))
                .or_else(|| {
                    let target = match provenance {
                        BindingProvenance::ValueAlias { target }
                        | BindingProvenance::BoundCallable { target, .. } => self
                            .symbol_path(target)
                            .and_then(|target| target.without_bind_suffix().or(Some(target)))?,
                        _ => return None,
                    };
                    target
                        .is_root()
                        .then(|| self.function_binding_at(target.to_string().as_str(), ident.span))
                        .flatten()
                })?;
        let function_end = self
            .function_spans()
            .find(|(candidate, _)| *candidate == function)
            .map(|(_, span)| span.hi);
        let reassigned = function_end.is_some_and(|end| {
            self.reassigned_between(scope, ident.sym.as_ref(), end, ident.span.lo)
        });
        (!reassigned).then_some(function)
    }

    /// Find a named function binding visible at a source position.
    pub(in crate::analysis) fn function_binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<FunctionId> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(function) = self.function_binding(scope, name) {
                return Some(function);
            }
            scope = self.scope_parent(scope)?;
        }
    }

    /// Find the smallest function span containing a source position.
    pub(in crate::analysis) fn function_id_for_span(&self, span: Span) -> Option<FunctionId> {
        self.function_spans()
            .filter_map(|(function, candidate)| {
                (candidate.lo <= span.lo && candidate.hi >= span.hi)
                    .then_some((candidate.hi.0 - candidate.lo.0, function))
            })
            .min_by_key(|(size, _)| *size)
            .map(|(_, function)| function)
    }
}
