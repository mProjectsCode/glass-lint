//! Structural scope graph types and collected alias facts.

use super::super::syntax::constant::ConstValue;
use super::super::syntax::{SymbolCallProvenance, SymbolMemberProvenance};
use super::super::value::{BindingId, BindingKey, BindingVersion, FunctionId, SymbolPath};
use std::collections::BTreeMap;
use swc_common::Span;

#[derive(Debug, Default, Clone)]
pub(in crate::analysis) struct ScopeGraph {
    pub(in crate::analysis::scope) environment: crate::Environment,
    pub(in crate::analysis::scope) scopes: Vec<AliasScope>,
    pub(in crate::analysis::scope) scopes_by_start: Vec<usize>,
    pub(in crate::analysis::scope) assignments:
        BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    pub(in crate::analysis::scope) binding_ids: BTreeMap<(usize, String), BindingId>,
    pub(in crate::analysis::scope) function_ids: BTreeMap<usize, FunctionId>,
    pub(in crate::analysis::scope) function_bindings: BTreeMap<(usize, String), FunctionId>,
    pub(in crate::analysis::scope) function_aliases: BTreeMap<(usize, String), FunctionId>,
    pub(in crate::analysis::scope) property_assignments:
        BTreeMap<(BindingKey, Vec<String>), Vec<PropertyAliasFact>>,
    pub(in crate::analysis::scope) rooted_property_mutations:
        BTreeMap<String, Vec<RootedPropertyMutationFact>>,
    pub(in crate::analysis::scope) parameter_aliases:
        BTreeMap<(FunctionId, String), BindingProvenance>,
    pub(in crate::analysis::scope) dynamic_evals: Vec<(usize, Span)>,
    pub(in crate::analysis::scope) mutable_static_objects:
        std::collections::BTreeSet<(usize, String)>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis::scope) struct RootedPropertyMutationFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) property: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct AliasScope {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) depth: usize,
    pub(in crate::analysis::scope) kind: ScopeKind,
    pub(in crate::analysis::scope) parent: Option<usize>,
    pub(in crate::analysis::scope) bindings: BTreeMap<String, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ScopeKind {
    Program,
    Function,
    Block,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum BindingProvenance {
    Local,
    ValueAlias {
        target: SymbolPath,
    },
    BoundCallable {
        target: SymbolPath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    BoundModuleCallable {
        module: String,
        export: String,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    ReturnedObject {
        source: SymbolPath,
    },
    ModuleExport {
        module: String,
        export: String,
    },
    ModuleNamespace {
        module: String,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(Vec<String>),
    StaticObjectValues(BTreeMap<String, SymbolPath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum BoundArgument {
    StaticString(String),
    RootedExpression(SymbolPath),
}

/// The collection boundary between lexical analysis and value interning.
///
/// Scope collection may use its compact declaration/assignment representation
/// internally, but the resolver receives one typed snapshot for each node. It
/// therefore does not need to reinterpret `BindingProvenance` while building
/// the authoritative value arena.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct IdentValueSeed {
    pub(in crate::analysis) call: SymbolCallProvenance,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) constant: ConstValue,
    pub(in crate::analysis) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct MemberValueSeed {
    pub(in crate::analysis) syntactic_chain: Option<SymbolPath>,
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis) binding: Option<BindingKey>,
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    pub(in crate::analysis) returned_member: Option<(SymbolPath, String)>,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct AliasAssignment {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) name: String,
    pub(in crate::analysis::scope) version: BindingVersion,
    pub(in crate::analysis::scope) provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct PropertyAliasFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: usize,
    pub(in crate::analysis::scope) target: Option<SymbolPath>,
}
