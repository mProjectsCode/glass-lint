use std::cell::Cell;

use glass_lint_datastructures::NameId;
use swc_common::Span;

use crate::analysis::model::scope::{
    BindingProvenance, LexicalScope, LexicalScopes, ScopeId, ScopeKind,
};

#[derive(Debug)]
pub(super) struct LexicalScopeIndex {
    scopes: LexicalScopes,
    scopes_by_start: Vec<ScopeId>,
    last_scope_query: Cell<Option<(Span, ScopeId)>>,
}

impl From<LexicalScopes> for LexicalScopeIndex {
    fn from(scopes: LexicalScopes) -> Self {
        let mut scopes_by_start: Vec<_> = (0..scopes.len()).map(ScopeId::new).collect();
        scopes_by_start.sort_by_key(|index| {
            let scope = scopes.get(*index).expect("scope index is allocated");
            (scope.span().lo, scope.depth())
        });
        Self {
            scopes,
            scopes_by_start,
            last_scope_query: Cell::new(None),
        }
    }
}

impl LexicalScopeIndex {
    pub(super) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.get(scope)?.parent()
    }

    pub(super) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.scopes.get(scope).map(LexicalScope::kind)
    }

    pub(super) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.scopes.get(scope).map(LexicalScope::span)
    }

    pub(super) fn scope_binding(&self, scope: ScopeId, name: NameId) -> Option<&BindingProvenance> {
        self.scopes.get(scope)?.binding(name)
    }

    pub(super) fn scope_at(&self, span: Span, scope_shape_valid: bool) -> ScopeId {
        if !scope_shape_valid {
            return ScopeId::new(0);
        }
        if let Some((cached_span, scope)) = self.last_scope_query.get()
            && cached_span == span
        {
            return scope;
        }
        let scope = self.find_scope_at(span);
        self.last_scope_query.set(Some((span, scope)));
        scope
    }

    fn find_scope_at(&self, span: Span) -> ScopeId {
        let position = self.scopes_by_start.partition_point(|index| {
            self.scopes
                .get(*index)
                .is_some_and(|scope| scope.span().lo <= span.lo)
        });
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return ScopeId::new(0);
        };
        while !self
            .scopes
            .get(scope)
            .is_some_and(|scope| scope.contains(span))
        {
            let Some(parent) = self.scopes.get(scope).and_then(LexicalScope::parent) else {
                return ScopeId::new(0);
            };
            scope = parent;
        }
        scope
    }
}
