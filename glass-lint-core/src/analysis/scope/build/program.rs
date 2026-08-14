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
    ScopeStackUnderflow,
    UnconsumedShape,
    InvalidBindingIndex,
    InvalidCheckpoint,
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

    pub(in crate::analysis) fn span(&self) -> Span {
        self.span
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn property(&self) -> &SymbolPath {
        &self.property
    }

    pub(in crate::analysis) fn receiver(&self) -> &swc_ecma_ast::Ident {
        &self.receiver
    }

    pub(in crate::analysis) fn take_target(self) -> Option<SymbolPath> {
        self.target
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

    pub(in crate::analysis) fn span(&self) -> Span {
        self.span
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn receiver(self) -> NamePath {
        self.receiver
    }

    pub(in crate::analysis) fn property(&self) -> Option<NameId> {
        self.property
    }
}
