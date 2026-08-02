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
}

#[derive(Debug, Clone)]
pub struct PropertyAliasAssignment {
    pub(crate) span: Span,
    pub(crate) scope: ScopeId,
    pub(crate) property: SymbolPath,
    pub(crate) receiver: swc_ecma_ast::Ident,
    pub(crate) target: Option<SymbolPath>,
}

#[derive(Debug, Clone)]
pub struct RootedPropertyMutation {
    pub(crate) span: Span,
    pub(crate) scope: ScopeId,
    pub(crate) receiver: NamePath,
    pub(crate) property: Option<NameId>,
}
