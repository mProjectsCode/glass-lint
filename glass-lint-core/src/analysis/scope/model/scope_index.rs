use std::cell::Cell;

use glass_lint_datastructures::NameId;
use swc_common::Span;

use crate::analysis::scope::{
    collect::aliases::contains,
    model::{
        id::ScopeId,
        types::{BindingProvenance, LexicalScope, ScopeKind},
    },
};

#[derive(Debug)]
pub(super) struct LexicalScopeIndex {
    pub(super) scopes: Vec<LexicalScope>,
    pub(super) scopes_by_start: Vec<ScopeId>,
    pub(super) last_scope_query: Cell<Option<(Span, ScopeId)>>,
}

impl LexicalScopeIndex {
    pub(super) fn new(scopes: Vec<LexicalScope>, scopes_by_start: Vec<ScopeId>) -> Self {
        Self {
            scopes,
            scopes_by_start,
            last_scope_query: Cell::new(None),
        }
    }

    pub(super) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.get(scope.index())?.parent
    }

    pub(super) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.scopes.get(scope.index()).map(|s| s.kind)
    }

    pub(super) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.scopes.get(scope.index()).map(|s| s.span)
    }

    pub(super) fn scope_binding(&self, scope: ScopeId, name: NameId) -> Option<&BindingProvenance> {
        self.scopes.get(scope.index())?.bindings.get(&name)
    }

    pub(super) fn scope_at(&self, span: Span, scope_shape_valid: bool) -> ScopeId {
        if !scope_shape_valid {
            return ScopeId::from(0);
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
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[index.index()].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return ScopeId::from(0);
        };
        while !contains(self.scopes[scope.index()].span, span) {
            let Some(parent) = self.scopes[scope.index()].parent else {
                return ScopeId::from(0);
            };
            scope = parent;
        }
        scope
    }
}
