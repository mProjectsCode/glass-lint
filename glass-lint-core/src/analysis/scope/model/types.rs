use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_common::Span;

use crate::analysis::{
    syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue},
    value::{BindingKey, BindingVersion},
};

use super::ScopeId;

#[derive(Debug, Clone)]
/// A rooted property write that may invalidate a global/member identity.
pub(in crate::analysis) struct RootedPropertyMutationFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) property: Option<NameId>,
}

#[derive(Debug, Clone)]
/// Lexical scope interval, kind, parent, and declaration bindings.
pub(in crate::analysis) struct LexicalScope {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) depth: usize,
    pub(in crate::analysis::scope) kind: ScopeKind,
    pub(in crate::analysis::scope) parent: Option<ScopeId>,
    pub(in crate::analysis::scope) bindings: BTreeMap<NameId, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Scope category relevant to JavaScript visibility and dynamic lookup.
pub(in crate::analysis) enum ScopeKind {
    Program,
    Function,
    Block,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Typed scope-level effects that invalidate later semantic assumptions.
pub(in crate::analysis) enum ScopeEffect {
    /// A proven direct dynamic-evaluation call occurred at this range.
    DynamicEvaluation { span: Span },
}

impl ScopeEffect {
    pub(super) fn span(&self) -> Span {
        match self {
            Self::DynamicEvaluation { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Conservative provenance attached to a lexical binding.
///
/// Each variant is produced during scope collection and consumed by the
/// resolver to build value identities. The resolver does not reinterpret
/// `BindingProvenance` after the value arena is built.
pub(in crate::analysis) enum BindingProvenance {
    /// A locally declared binding (`var`, `let`, `const`, `function`,
    /// `class`, or parameter). Produced for declarations that do not
    /// match a more specific pattern. Consumed by the resolver to build
    /// `ValueId::Local`.
    Local,
    /// A binding initialized to a tracked value reference
    /// (`const x = y` where `y` has a proven identity). Produced during
    /// assignment collection. Consumed by the resolver to redirect the
    /// binding to the target's value ID.
    ValueAlias { target: NamePath },
    /// A binding initialized to a callable with bound arguments
    /// (`const bound = fn.bind(obj)`). Produced during assignment
    /// collection. Consumed by the resolver to build a value identity
    /// preserving the bound arguments.
    BoundCallable {
        target: NamePath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    /// A binding initialized to a module export with bound arguments.
    /// Produced during assignment collection. Consumed by the resolver.
    BoundModuleCallable {
        module: SmolStr,
        export: SmolStr,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    /// A binding capturing the return value of a tracked callable
    /// (`const x = fetch(url)`). Produced during assignment collection.
    /// Consumed by the resolver.
    ReturnedObject { source: NamePath },
    /// A binding aliasing a named module export
    /// (`const { send } = require("http")` or equivalent import).
    /// Produced during scope collection. Consumed by the resolver to
    /// build `ValueId::ModuleExport`.
    ModuleExport { module: SmolStr, export: SmolStr },
    /// A binding capturing an entire module namespace
    /// (`const fs = require("fs")`). Produced during scope collection.
    /// Consumed by the resolver to build `ValueId::ModuleNamespace`.
    ModuleNamespace { module: SmolStr },
    /// A binding initialized to a string literal. Produced during
    /// assignment collection. Consumed by the resolver.
    StaticString(String),
    /// A binding initialized to a number literal. Produced during
    /// assignment collection. Consumed by the resolver.
    StaticNumber(usize),
    /// A binding initialized to an array of string literals. Produced
    /// during assignment collection. Consumed by the resolver.
    StaticStringArray(Vec<String>),
    /// A binding initialized to an object whose keys are all static
    /// strings. Produced during assignment collection. Consumed by the
    /// resolver.
    StaticObjectKeys(Vec<NameId>),
    /// A binding initialized to an object whose values are all tracked
    /// value references. Produced during assignment collection. Consumed
    /// by the resolver.
    StaticObjectValues(BTreeMap<NameId, NamePath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Static argument identity preserved by a modeled callable bind.
pub(in crate::analysis) enum BoundArgument {
    StaticString(String),
    RootedExpression(NamePath),
}

/// The collection boundary between lexical analysis and value interning.
///
/// Scope collection may use its compact declaration/assignment representation
/// internally, but the resolver receives one typed snapshot for each node. It
/// therefore does not need to reinterpret `BindingProvenance` while building
/// the authoritative value arena.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct IdentValueSeed {
    /// Call provenance for the identifier at its use position.
    pub(in crate::analysis) call: SymbolCallProvenance,
    /// Rooted member path, when callable identity is proven.
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    /// Versioned lexical binding identity.
    pub(in crate::analysis) binding: Option<BindingKey>,
    /// Bounded constant value, or unknown.
    pub(in crate::analysis) constant: ConstValue,
    /// Static arguments captured by a supported `.bind()` call.
    pub(in crate::analysis) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
/// Resolver inputs derived from one member expression.
pub(in crate::analysis) struct MemberValueSeed {
    /// Syntax-only member spelling retained for diagnostics/indexing.
    pub(in crate::analysis) syntactic_chain: Option<SymbolPath>,
    /// Proven rooted path after alias and mutation checks.
    pub(in crate::analysis) rooted_chain: Option<NamePath>,
    /// Versioned receiver/property binding identity.
    pub(in crate::analysis) binding: Option<BindingKey>,
    /// Imported namespace/member provenance, when known.
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    /// Returned-object source and member name, when tracked.
    pub(in crate::analysis) returned_member: Option<(NamePath, NamePath)>,
}

#[derive(Debug, Clone)]
/// One source-ordered reassignment of a lexical binding.
pub(in crate::analysis) struct AliasAssignment {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) name: NameId,
    pub(in crate::analysis::scope) version: BindingVersion,
    pub(in crate::analysis::scope) provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
/// One rooted property assignment indexed by receiver and path.
pub(in crate::analysis) struct PropertyAliasFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) target: Option<SymbolPath>,
}
