use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use swc_common::Span;

use crate::analysis::scope::{FrozenScopeGraph, ScopeId};

pub(in crate::analysis) struct ScopedProgram {
    pub(crate) graph: FrozenScopeGraph,
    pub(crate) issues: Vec<ScopeCollectionIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ScopeCollectionIssue {
    ShapeMismatch,
    UnconsumedShape,
    InvalidBindingIndex,
}

#[derive(Debug, Clone)]
pub struct PropertyAliasAssignment {
    span: Span,
    scope: ScopeId,
    property: SymbolPath,
    receiver: swc_ecma_ast::Ident,
    target: Option<SymbolPath>,
}

impl PropertyAliasAssignment {
    pub(super) fn new(
        span: Span,
        scope: ScopeId,
        property: SymbolPath,
        receiver: swc_ecma_ast::Ident,
        target: Option<SymbolPath>,
    ) -> Self {
        Self {
            span,
            scope,
            property,
            receiver,
            target,
        }
    }

    pub(in crate::analysis) fn into_parts(
        self,
    ) -> (
        Span,
        ScopeId,
        SymbolPath,
        swc_ecma_ast::Ident,
        Option<SymbolPath>,
    ) {
        (
            self.span,
            self.scope,
            self.property,
            self.receiver,
            self.target,
        )
    }
}

#[derive(Debug, Clone)]
pub struct RootedPropertyMutation {
    span: Span,
    scope: ScopeId,
    receiver: NamePath,
    property: Option<NameId>,
}

impl RootedPropertyMutation {
    pub(super) fn new(
        span: Span,
        scope: ScopeId,
        receiver: NamePath,
        property: Option<NameId>,
    ) -> Self {
        Self {
            span,
            scope,
            receiver,
            property,
        }
    }

    pub(in crate::analysis) fn into_parts(self) -> (Span, ScopeId, NamePath, Option<NameId>) {
        (self.span, self.scope, self.receiver, self.property)
    }
}
