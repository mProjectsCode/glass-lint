//! Structural scope graph types and collected alias facts.
//!
//! IDs are assigned after collection and are stable within one analyzed
//! module. Assignment versions and source spans remain part of the query
//! contract so aliases cannot cross a reassignment or lexical boundary.

use std::collections::BTreeMap;

use swc_common::{BytePos, Span};

use super::super::{
    syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue},
    value::{BindingId, BindingKey, BindingVersion, FunctionId, SymbolPath},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identity of a lexical scope within one analyzed module.
pub(in crate::analysis) struct ScopeId(usize);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A name resolved within one lexical scope.
pub(in crate::analysis) struct ScopedName {
    scope: ScopeId,
    name: String,
}

impl ScopedName {
    pub(in crate::analysis) fn new(scope: ScopeId, name: impl Into<String>) -> Self {
        Self {
            scope,
            name: name.into(),
        }
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn name(&self) -> &str {
        &self.name
    }
}

impl ScopeId {
    pub(in crate::analysis) fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for ScopeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

#[derive(Debug, Default, Clone)]
/// Immutable lexical scope/index model consumed by resolution queries.
pub(in crate::analysis) struct ScopeGraph {
    /// Host globals and member capabilities used for unshadowed checks.
    environment: crate::Environment,
    /// Lexical scopes in predeclaration order.
    scopes: Vec<AliasScope>,
    /// Scope indexes sorted by opening position for position lookup.
    scopes_by_start: Vec<ScopeId>,
    /// Source-ordered assignments grouped by scope and name.
    assignments: BTreeMap<ScopeId, BTreeMap<String, Vec<AliasAssignment>>>,
    /// Stable binding IDs keyed by lexical scope and name.
    binding_ids: BTreeMap<ScopedName, BindingId>,
    /// Stable function IDs keyed by function scope.
    function_ids: BTreeMap<ScopeId, FunctionId>,
    /// Direct function declarations visible from a scope.
    function_bindings: BTreeMap<ScopedName, FunctionId>,
    /// Aliases to locally declared functions.
    function_aliases: BTreeMap<ScopedName, FunctionId>,
    /// Property writes indexed by versioned receiver and path.
    property_assignments: BTreeMap<(BindingKey, Vec<String>), Vec<PropertyAliasFact>>,
    /// Rooted writes that invalidate member identities.
    rooted_property_mutations: BTreeMap<String, Vec<RootedPropertyMutationFact>>,
    /// Proven parameter identities shared by compatible call sites.
    parameter_aliases: BTreeMap<(FunctionId, String), BindingProvenance>,
    /// Dynamic-evaluation sites that invalidate later lexical assumptions.
    dynamic_evals: Vec<(ScopeId, ScopeEffect)>,
    /// Dynamic-evaluation spans grouped by scope for indexed queries.
    dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    /// Static objects whose `var` binding may be mutated.
    mutable_static_objects: std::collections::BTreeSet<ScopedName>,
}

impl ScopeGraph {
    /// Convert collector-side property events into sorted query indexes.
    pub(super) fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<super::collect::PropertyAliasAssignment>,
        rooted_mutations: Vec<super::collect::RootedPropertyMutation>,
        dynamic_evals: Vec<(ScopeId, ScopeEffect)>,
    ) {
        for assignment in property_assignments {
            let Some(receiver_key) = self
                .binding_key_for_name(assignment.receiver.sym.as_ref(), assignment.receiver.span)
            else {
                continue;
            };
            self.property_assignments
                .entry((
                    receiver_key,
                    assignment
                        .property
                        .strip_prefix(assignment.receiver.sym.as_ref())
                        .and_then(|path| path.strip_prefix('.'))
                        .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
                        .unwrap_or_default(),
                ))
                .or_default()
                .push(PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target.map(std::convert::Into::into),
                });
        }
        for assignments in self.property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        for mutation in rooted_mutations {
            self.rooted_property_mutations
                .entry(mutation.receiver)
                .or_default()
                .push(RootedPropertyMutationFact {
                    span: mutation.span,
                    scope: mutation.scope,
                    property: mutation.property,
                });
        }
        for mutations in self.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.lo);
        }
        self.dynamic_evals = dynamic_evals
            .into_iter()
            .filter(|(_, effect)| self.binding_at("eval", effect.span()).is_none())
            .collect();
        self.dynamic_evals
            .sort_by_key(|(_, effect)| effect.span().hi);
        self.dynamic_evals_by_scope.clear();
        for (scope, effect) in &self.dynamic_evals {
            self.dynamic_evals_by_scope
                .entry(*scope)
                .or_default()
                .push(effect.clone());
        }
        for spans in self.dynamic_evals_by_scope.values_mut() {
            spans.sort_by_key(|effect| effect.span().hi);
        }
    }

    pub(super) fn is_global(&self, name: &str) -> bool {
        self.environment.is_global(name)
    }

    pub(super) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.environment.is_global_member(root, member)
    }

    /// Find the latest assignment at or before a source position.
    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: &str,
        span: Span,
    ) -> Option<&AliasAssignment> {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
            })
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: &str) -> Option<BindingId> {
        self.binding_ids.get(&ScopedName::new(scope, name)).copied()
    }

    pub(super) fn parameter_alias_for(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<&BindingProvenance> {
        self.function_ids
            .get(&scope)
            .and_then(|function| self.parameter_aliases.get(&(*function, name.to_string())))
    }

    pub(in crate::analysis) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.get(scope.index())?.parent
    }

    pub(super) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.scopes.get(scope.index()).map(|scope| scope.kind)
    }

    pub(super) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.scopes.get(scope.index()).map(|scope| scope.span)
    }

    pub(super) fn scope_binding(&self, scope: ScopeId, name: &str) -> Option<&BindingProvenance> {
        self.scopes.get(scope.index())?.bindings.get(name)
    }

    /// Assemble the immutable graph before property indexes are attached.
    pub(super) fn from_parts(parts: ScopeGraphParts) -> Self {
        Self {
            environment: parts.environment,
            scopes: parts.scopes,
            scopes_by_start: parts.scopes_by_start,
            assignments: parts.assignments,
            binding_ids: parts.binding_ids,
            function_ids: parts.function_ids,
            function_bindings: parts.function_bindings,
            function_aliases: parts.function_aliases,
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            parameter_aliases: parts.parameter_aliases,
            dynamic_evals: Vec::new(),
            dynamic_evals_by_scope: BTreeMap::new(),
            mutable_static_objects: parts.mutable_static_objects,
        }
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.function_ids.get(&scope).copied()
    }

    pub(super) fn function_spans(&self) -> impl Iterator<Item = (FunctionId, Span)> + '_ {
        self.function_ids.iter().filter_map(|(scope, function)| {
            self.scopes
                .get(scope.index())
                .map(|scope| (*function, scope.span))
        })
    }

    pub(super) fn function_binding(&self, scope: ScopeId, name: &str) -> Option<FunctionId> {
        self.function_bindings
            .get(&ScopedName::new(scope, name))
            .copied()
    }

    pub(super) fn function_alias(&self, scope: ScopeId, name: &str) -> Option<FunctionId> {
        self.function_aliases
            .get(&ScopedName::new(scope, name))
            .copied()
    }

    pub(super) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: &str,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .is_some_and(|assignments| {
                assignments
                    .iter()
                    .any(|assignment| assignment.span.lo > start && assignment.span.lo <= end)
            })
    }

    pub(super) fn binding_version(&self, scope: ScopeId, name: &str, span: Span) -> BindingVersion {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
            })
            .map_or(BindingVersion(0), |assignment| assignment.version)
    }

    pub(super) fn property_aliases(
        &self,
        key: &(BindingKey, Vec<String>),
    ) -> Option<&[PropertyAliasFact]> {
        self.property_assignments.get(key).map(Vec::as_slice)
    }

    pub(super) fn rooted_mutations(&self, root: &str) -> Option<&[RootedPropertyMutationFact]> {
        self.rooted_property_mutations.get(root).map(Vec::as_slice)
    }

    pub(super) fn is_mutable_static_object(&self, scope: ScopeId, name: &str) -> bool {
        self.mutable_static_objects
            .contains(&ScopedName::new(scope, name))
    }

    pub(super) fn has_eval_after(&self, scope: ScopeId, span: Span) -> bool {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if let Some(evals) = self.dynamic_evals_by_scope.get(&scope)
                && evals.partition_point(|effect| effect.span().hi < span.lo) > 0
            {
                return true;
            }
            current = self.scope_parent(scope);
        }
        false
    }

    /// Find the innermost lexical scope containing a source span.
    pub(in crate::analysis) fn scope_at(&self, span: Span) -> ScopeId {
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[index.index()].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return ScopeId::from(0);
        };
        while !span_contains(self.scopes[scope.index()].span, span) {
            let Some(parent) = self.scopes[scope.index()].parent else {
                return ScopeId::from(0);
            };
            scope = parent;
        }
        scope
    }
}

/// Owned inputs used to assemble a collected [`ScopeGraph`].
pub(super) struct ScopeGraphParts {
    pub(super) environment: crate::Environment,
    pub(super) scopes: Vec<AliasScope>,
    pub(super) scopes_by_start: Vec<ScopeId>,
    pub(super) assignments: BTreeMap<ScopeId, BTreeMap<String, Vec<AliasAssignment>>>,
    pub(super) binding_ids: BTreeMap<ScopedName, BindingId>,
    pub(super) function_ids: BTreeMap<ScopeId, FunctionId>,
    pub(super) function_bindings: BTreeMap<ScopedName, FunctionId>,
    pub(super) function_aliases: BTreeMap<ScopedName, FunctionId>,
    pub(super) parameter_aliases: BTreeMap<(FunctionId, String), BindingProvenance>,
    pub(super) mutable_static_objects: std::collections::BTreeSet<ScopedName>,
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.lo <= inner.lo && outer.hi >= inner.hi
}

#[derive(Debug, Clone)]
/// A rooted property write that may invalidate a global/member identity.
pub(in crate::analysis::scope) struct RootedPropertyMutationFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) property: Option<String>,
}

#[derive(Debug, Clone)]
/// Lexical scope interval, kind, parent, and declaration bindings.
pub(in crate::analysis) struct AliasScope {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) depth: usize,
    pub(in crate::analysis::scope) kind: ScopeKind,
    pub(in crate::analysis::scope) parent: Option<ScopeId>,
    pub(in crate::analysis::scope) bindings: BTreeMap<String, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    fn span(&self) -> Span {
        match self {
            Self::DynamicEvaluation { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Conservative provenance attached to a lexical binding.
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
/// Static argument identity preserved by a modeled callable bind.
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
    pub(in crate::analysis) rooted_chain: Option<SymbolPath>,
    /// Versioned receiver/property binding identity.
    pub(in crate::analysis) binding: Option<BindingKey>,
    /// Imported namespace/member provenance, when known.
    pub(in crate::analysis) module_member: Option<SymbolMemberProvenance>,
    /// Returned-object source and member name, when tracked.
    pub(in crate::analysis) returned_member: Option<(SymbolPath, String)>,
}

#[derive(Debug, Clone)]
/// One source-ordered reassignment of a lexical binding.
pub(in crate::analysis) struct AliasAssignment {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) name: String,
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
