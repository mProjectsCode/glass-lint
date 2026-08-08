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

pub(in crate::analysis) struct PropertyAliasAssignmentData {
    pub(in crate::analysis) span: Span,
    pub(in crate::analysis) scope: ScopeId,
    pub(in crate::analysis) property: SymbolPath,
    pub(in crate::analysis) receiver: swc_ecma_ast::Ident,
    pub(in crate::analysis) target: Option<SymbolPath>,
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

    pub(in crate::analysis) fn into_data(self) -> PropertyAliasAssignmentData {
        PropertyAliasAssignmentData {
            span: self.span,
            scope: self.scope,
            property: self.property,
            receiver: self.receiver,
            target: self.target,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RootedPropertyMutation {
    span: Span,
    scope: ScopeId,
    receiver: NamePath,
    property: Option<NameId>,
}

pub(in crate::analysis) struct RootedPropertyMutationData {
    pub(in crate::analysis) span: Span,
    pub(in crate::analysis) scope: ScopeId,
    pub(in crate::analysis) receiver: NamePath,
    pub(in crate::analysis) property: Option<NameId>,
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

    pub(in crate::analysis) fn into_data(self) -> RootedPropertyMutationData {
        RootedPropertyMutationData {
            span: self.span,
            scope: self.scope,
            receiver: self.receiver,
            property: self.property,
        }
    }
}
