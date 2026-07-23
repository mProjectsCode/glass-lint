//! Structural scope graph types and collected alias facts.
//!
//! IDs are assigned after collection and are stable within one analyzed
//! module. Assignment versions and source spans remain part of the query
//! contract so aliases cannot cross a reassignment or lexical boundary.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap},
};

use smol_str::SmolStr;
use swc_common::{BytePos, Span};

use crate::{
    Environment,
    analysis::{
        name::{NameId, NameTable},
        scope::collect::{PropertyAliasAssignment, RootedPropertyMutation, aliases::contains},
        syntax::{SymbolCallProvenance, SymbolMemberProvenance, constant::ConstValue},
        value::{
            BindingId, BindingKey, BindingRoot, BindingVersion, FunctionId, NamePath, SymbolPath,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identity of a lexical scope within one analyzed module.
pub(in crate::analysis) struct ScopeId(usize);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A name resolved within one lexical scope.
pub(in crate::analysis) struct ScopedName {
    scope: ScopeId,
    name: NameId,
}

impl ScopedName {
    pub(in crate::analysis) fn new(scope: ScopeId, name: NameId) -> Self {
        Self { scope, name }
    }

    pub(in crate::analysis) fn scope(&self) -> ScopeId {
        self.scope
    }

    pub(in crate::analysis) fn name(&self) -> NameId {
        self.name
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

// ---------------------------------------------------------------------------
// Sub-structs splitting owned concerns from the monolithic ScopeGraph
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(super) struct NameEnvironment {
    pub(super) names: NameTable,
    pub(super) environment: Environment,
}

impl NameEnvironment {
    pub(super) fn new(names: NameTable, environment: Environment) -> Self {
        Self { names, environment }
    }

    pub(super) fn resolve_name_id(&self, name: NameId) -> Option<SmolStr> {
        self.names.resolve(name).map(SmolStr::new)
    }

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.names.lookup(name)
    }

    pub(super) fn intern_name_mut(&mut self, name: &str) -> Option<NameId> {
        self.names.intern(name).ok()
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.lookup_path(path)
    }

    pub(super) fn name_table_exhausted(&self) -> bool {
        self.names.exhausted()
    }

    pub(super) fn into_name_table(self) -> NameTable {
        self.names
    }

    pub(super) fn name_table_mut(&mut self) -> &mut NameTable {
        &mut self.names
    }

    pub(super) fn name_exhaustion(&self) -> Option<crate::analysis::name::NameExhausted> {
        self.names.exhaustion()
    }

    #[cfg(test)]
    pub(super) fn name_snapshot(&self) -> NameTable {
        self.names.clone()
    }

    pub(super) fn symbol_path(&self, path: &NamePath) -> Option<SymbolPath> {
        self.names.resolve_path(path)
    }

    pub(super) fn is_global(&self, name: &str) -> bool {
        self.environment.is_global(name)
    }

    pub(super) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.environment.is_global_member(root, member)
    }

    pub(super) fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.environment.global_objects()
    }
}

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

#[derive(Debug)]
pub(super) struct BindingIndex {
    pub(super) assignments: FrozenAssignmentIndex,
    pub(super) binding_ids: BTreeMap<ScopedName, BindingId>,
    pub(super) function_ids: BTreeMap<ScopeId, FunctionId>,
    pub(super) function_bindings: BTreeMap<ScopedName, FunctionId>,
    pub(super) function_aliases: BTreeMap<ScopedName, FunctionId>,
    pub(super) parameter_aliases: BTreeMap<(FunctionId, NameId), BindingProvenance>,
}

impl BindingIndex {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        assignments: FrozenAssignmentIndex,
        binding_ids: BTreeMap<ScopedName, BindingId>,
        function_ids: BTreeMap<ScopeId, FunctionId>,
        function_bindings: BTreeMap<ScopedName, FunctionId>,
        function_aliases: BTreeMap<ScopedName, FunctionId>,
        parameter_aliases: BTreeMap<(FunctionId, NameId), BindingProvenance>,
    ) -> Self {
        Self {
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            parameter_aliases,
        }
    }

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> Option<&AliasAssignment> {
        self.assignments.latest_at(scope, name, span)
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: NameId) -> Option<BindingId> {
        self.binding_ids.get(&ScopedName::new(scope, name)).copied()
    }

    pub(super) fn parameter_alias_for(
        &self,
        function: FunctionId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.parameter_aliases.get(&(function, name))
    }

    pub(super) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.assignments.changed_between(scope, name, start, end)
    }

    pub(super) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.assignments.version_at(scope, name, span)
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.function_ids.get(&scope).copied()
    }

    pub(super) fn function_spans<'a>(
        &'a self,
        scopes: &'a LexicalScopeIndex,
    ) -> impl Iterator<Item = (FunctionId, Span)> + 'a {
        self.function_ids
            .iter()
            .filter_map(move |(scope, function)| {
                scopes.scope_span(*scope).map(|span| (*function, span))
            })
    }

    pub(super) fn function_binding(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.function_bindings
            .get(&ScopedName::new(scope, name))
            .copied()
    }

    pub(super) fn function_alias(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.function_aliases
            .get(&ScopedName::new(scope, name))
            .copied()
    }
}

#[derive(Debug)]
pub(super) struct MutationIndex {
    pub(super) property_assignments: BTreeMap<(BindingKey, NamePath), Vec<PropertyAliasFact>>,
    pub(super) rooted_property_mutations: BTreeMap<NamePath, Vec<RootedPropertyMutationFact>>,
    pub(super) dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    pub(super) mutable_static_objects: BTreeSet<ScopedName>,
}

impl MutationIndex {
    pub(super) fn new(mutable_static_objects: BTreeSet<ScopedName>) -> Self {
        Self {
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            dynamic_evals_by_scope: BTreeMap::new(),
            mutable_static_objects,
        }
    }

    pub(super) fn property_aliases(
        &self,
        key: &(BindingKey, NamePath),
    ) -> Option<&[PropertyAliasFact]> {
        self.property_assignments.get(key).map(Vec::as_slice)
    }

    pub(super) fn rooted_mutations(
        &self,
        root: &NamePath,
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.rooted_property_mutations.get(root).map(Vec::as_slice)
    }

    pub(super) fn is_mutable_static_object(&self, scope: ScopeId, name: NameId) -> bool {
        self.mutable_static_objects
            .contains(&ScopedName::new(scope, name))
    }
}

// ---------------------------------------------------------------------------
// ScopeGraph — mutable collection-phase struct
// ---------------------------------------------------------------------------

#[derive(Debug)]
/// Mutable scope graph used during the collection phase.
///
/// After calling [`finish_collected_properties`] and [`freeze`], callers
/// receive a read-only [`FrozenScopeGraph`] for all query operations.
pub(in crate::analysis) struct ScopeGraph {
    pub(super) names: NameEnvironment,
    pub(super) scopes: LexicalScopeIndex,
    pub(super) bindings: BindingIndex,
    pub(super) mutations: MutationIndex,
    /// False when source-order collection did not consume the planned shape.
    scope_shape_valid: bool,
    /// Lazy cache for member chain provenance queries.
    pub(super) member_cache: MemberChainCache,
}

#[derive(Debug)]
/// Read-only scope graph produced by freezing a [`ScopeGraph`].
///
/// All query methods (provenance, bindings, constants, functions, rooted)
/// are defined on this type.  The collection/building phase produces a
/// `ScopeGraph`, then calls `freeze()` to obtain a `FrozenScopeGraph` for
/// the resolver.
pub(in crate::analysis) struct FrozenScopeGraph {
    pub(super) names: NameEnvironment,
    pub(super) scopes: LexicalScopeIndex,
    pub(super) bindings: BindingIndex,
    pub(super) mutations: MutationIndex,
    /// Lazy cache for member chain provenance queries.
    pub(super) member_cache: MemberChainCache,
}

impl ScopeGraph {
    /// Create a minimally-initialized scope graph for test use.
    #[cfg(test)]
    pub(in crate::analysis) fn create_for_test(names: NameTable) -> Self {
        Self {
            names: NameEnvironment::new(names, Environment::default()),
            scopes: LexicalScopeIndex::new(Vec::new(), Vec::new()),
            bindings: BindingIndex::new(
                FrozenAssignmentIndex::from_assignments(Vec::new()),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
            ),
            mutations: MutationIndex::new(BTreeSet::new()),
            scope_shape_valid: true,
            member_cache: MemberChainCache::default(),
        }
    }

    /// Assemble the immutable graph before property indexes are attached.
    pub(super) fn from_parts(parts: ScopeGraphParts) -> Self {
        Self {
            names: NameEnvironment::new(parts.names, parts.environment),
            scopes: LexicalScopeIndex::new(parts.scopes, parts.scopes_by_start),
            bindings: BindingIndex::new(
                parts.assignments,
                parts.binding_ids,
                parts.function_ids,
                parts.function_bindings,
                parts.function_aliases,
                parts.parameter_aliases,
            ),
            mutations: MutationIndex::new(parts.mutable_static_objects),
            scope_shape_valid: parts.scope_shape_valid,
            member_cache: MemberChainCache::default(),
        }
    }

    /// Freeze this scope graph into a read-only query graph.
    pub fn freeze(self) -> FrozenScopeGraph {
        FrozenScopeGraph {
            names: self.names,
            scopes: self.scopes,
            bindings: self.bindings,
            mutations: self.mutations,
            member_cache: self.member_cache,
        }
    }

    // -- Name-related helpers kept on ScopeGraph for collection --

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.names.name_id(name)
    }

    pub(in crate::analysis) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.name_path(path)
    }

    // -- Lexical-scope helpers on ScopeGraph --

    pub(in crate::analysis) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.scope_parent(scope)
    }

    pub(super) fn scope_binding(&self, scope: ScopeId, name: &str) -> Option<&BindingProvenance> {
        let name = self.name_id(name)?;
        self.scopes.scope_binding(scope, name)
    }

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> ScopeId {
        self.scopes.scope_at(span, self.scope_shape_valid)
    }

    // -- Binding helpers on ScopeGraph --

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: &str,
        span: Span,
    ) -> Option<&AliasAssignment> {
        let name = self.name_id(name)?;
        self.bindings.assignment_at(scope, name, span)
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: &str) -> Option<BindingId> {
        let name = self.name_id(name)?;
        self.bindings.binding_id_at(scope, name)
    }

    pub(super) fn parameter_alias_for(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> Option<&BindingProvenance> {
        let function = self.bindings.function_for_scope(scope)?;
        let name = self.name_id(name)?;
        self.bindings.parameter_alias_for(function, name)
    }

    pub(super) fn binding_version(&self, scope: ScopeId, name: &str, span: Span) -> BindingVersion {
        let Some(name) = self.name_id(name) else {
            return BindingVersion(0);
        };
        self.bindings.binding_version(scope, name, span)
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.bindings.function_for_scope(scope)
    }

    /// Convert collector-side property events into sorted query indexes.
    pub(super) fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<PropertyAliasAssignment>,
        rooted_mutations: Vec<RootedPropertyMutation>,
        dynamic_evals: Vec<(ScopeId, ScopeEffect)>,
    ) {
        for assignment in property_assignments {
            let Some(receiver_key) = self
                .binding_key_for_name(assignment.receiver.sym.as_ref(), assignment.receiver.span)
            else {
                continue;
            };
            let path = assignment
                .property
                .without_first_segment()
                .and_then(|path| self.name_path(&path))
                .unwrap_or_default();
            self.mutations
                .property_assignments
                .entry((receiver_key, path))
                .or_default()
                .push(PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target,
                });
        }
        for assignments in self.mutations.property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        for mutation in rooted_mutations {
            self.mutations
                .rooted_property_mutations
                .entry(mutation.receiver)
                .or_default()
                .push(RootedPropertyMutationFact {
                    span: mutation.span,
                    scope: mutation.scope,
                    property: mutation.property,
                });
        }
        for mutations in self.mutations.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.lo);
        }
        let mut evals: Vec<(ScopeId, ScopeEffect)> = dynamic_evals
            .into_iter()
            .filter(|(_, effect)| self.binding_at("eval", effect.span()).is_none())
            .collect();
        evals.sort_by_key(|(_, effect)| effect.span().hi);
        self.mutations.dynamic_evals_by_scope.clear();
        for (scope, effect) in evals {
            self.mutations
                .dynamic_evals_by_scope
                .entry(scope)
                .or_default()
                .push(effect);
        }
        for spans in self.mutations.dynamic_evals_by_scope.values_mut() {
            spans.sort_by_key(|effect| effect.span().hi);
        }
    }

    // -- Query methods needed during collection (also on FrozenScopeGraph) --

    /// Resolve the binding provenance visible at a use position.
    pub(super) fn binding_at(&self, name: &str, span: Span) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        self.assignment_at(scope, name, span)
            .map(|assignment| &assignment.provenance)
            .or_else(|| self.parameter_alias_for(scope, name))
            .or(Some(declaration))
    }

    /// Find the nearest lexical declaration and its owning scope.
    fn binding_with_scope_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scope_binding(scope, name) {
                return Some((scope, binding));
            }
            scope = self.scope_parent(scope)?;
        }
    }

    /// Build a stable key for a name, using a global root when unbound.
    fn binding_key_for_name(&self, name: &str, span: Span) -> Option<BindingKey> {
        if let Some((scope, _)) = self.binding_with_scope_at(name, span) {
            return Some(BindingKey::new(BindingRoot::Binding {
                function: self.function_scope_at(scope),
                binding: self.binding_id_at(scope, name)?,
                version: self.binding_version_at(scope, name, span),
            }));
        }
        Some(BindingKey::new(BindingRoot::Global(name.to_string())))
    }

    fn binding_version_at(&self, scope: ScopeId, name: &str, span: Span) -> BindingVersion {
        self.binding_version(scope, name, span)
    }

    fn function_scope_at(&self, scope: ScopeId) -> FunctionId {
        let mut current = Some(scope);
        while let Some(index) = current {
            if let Some(function) = self.function_for_scope(index) {
                return function;
            }
            current = self.scope_parent(index);
        }
        FunctionId(0)
    }
}

/// Owned inputs used to assemble a collected [`ScopeGraph`].
pub(super) struct ScopeGraphParts {
    pub(super) environment: Environment,
    pub(super) names: NameTable,
    pub(super) scopes: Vec<LexicalScope>,
    pub(super) scopes_by_start: Vec<ScopeId>,
    pub(super) assignments: FrozenAssignmentIndex,
    pub(super) binding_ids: BTreeMap<ScopedName, BindingId>,
    pub(super) function_ids: BTreeMap<ScopeId, FunctionId>,
    pub(super) function_bindings: BTreeMap<ScopedName, FunctionId>,
    pub(super) function_aliases: BTreeMap<ScopedName, FunctionId>,
    pub(super) parameter_aliases: BTreeMap<(FunctionId, NameId), BindingProvenance>,
    pub(super) mutable_static_objects: BTreeSet<ScopedName>,
    pub(super) scope_shape_valid: bool,
}

#[derive(Debug, Clone)]
/// A rooted property write that may invalidate a global/member identity.
pub(in crate::analysis::scope) struct RootedPropertyMutationFact {
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
    fn span(&self) -> Span {
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

#[derive(Debug, Default)]
/// Lazy cache for member chain provenance queries on the frozen scope graph.
pub(in crate::analysis) struct MemberChainCache {
    /// Resolved member chain results keyed by member expression span.
    pub(in crate::analysis) resolve_chain: RefCell<HashMap<Span, Option<SymbolPath>>>,
    /// Rooted chain mutation results keyed by (chain, span).
    pub(in crate::analysis) mutated_at: RefCell<HashMap<(SmolStr, Span), bool>>,
    /// Complete member value seed results keyed by member expression span.
    pub(in crate::analysis) member_seed: RefCell<HashMap<Span, MemberValueSeed>>,
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

/// Source-ordered assignment history frozen after collection.
///
/// All inner `Vec<AliasAssignment>` values are sorted by `span.lo`; this
/// invariant is established during construction and never violated.
#[derive(Debug, Clone)]
pub(super) struct FrozenAssignmentIndex {
    inner: BTreeMap<ScopeId, BTreeMap<NameId, Vec<AliasAssignment>>>,
}

impl FrozenAssignmentIndex {
    /// Build from a flat, unsorted assignment stream.
    /// Sorts and groups by (scope, name) during construction.
    pub(super) fn from_assignments(assignments: Vec<AliasAssignment>) -> Self {
        let mut inner: BTreeMap<ScopeId, BTreeMap<NameId, Vec<AliasAssignment>>> = BTreeMap::new();
        for assignment in assignments {
            inner
                .entry(assignment.scope)
                .or_default()
                .entry(assignment.name)
                .or_default()
                .push(assignment);
        }
        for scope_entries in inner.values_mut() {
            for binding_assignments in scope_entries.values_mut() {
                binding_assignments.sort_by_key(|a| a.span.lo);
            }
        }
        Self { inner }
    }

    /// Retrieve the sorted slice for one scope/name pair, if it exists.
    fn get(&self, scope: ScopeId, name: NameId) -> Option<&[AliasAssignment]> {
        self.inner.get(&scope)?.get(&name).map(Vec::as_slice)
    }

    /// Find the index of the latest assignment at or before `span.lo`.
    fn latest_index(assignments: &[AliasAssignment], span: Span) -> Option<usize> {
        let idx = assignments.partition_point(|a| a.span.lo <= span.lo);
        idx.checked_sub(1)
    }

    /// Latest assignment at or before a source position.
    pub(super) fn latest_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> Option<&AliasAssignment> {
        let assignments = self.get(scope, name)?;
        let idx = Self::latest_index(assignments, span)?;
        Some(&assignments[idx])
    }

    /// Binding version visible at a source position.
    pub(super) fn version_at(&self, scope: ScopeId, name: NameId, span: Span) -> BindingVersion {
        self.latest_at(scope, name, span)
            .map_or(BindingVersion(0), |a| a.version)
    }

    /// Whether any assignment occurred in the half-open interval `(start,
    /// end]`.
    pub(super) fn changed_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        let Some(assignments) = self.get(scope, name) else {
            return false;
        };
        let after_start = assignments.partition_point(|a| a.span.lo <= start);
        after_start < assignments.len() && assignments[after_start].span.lo <= end
    }
}

// ---------------------------------------------------------------------------
// FrozenScopeGraph — all query methods delegate to the sub-structs
// ---------------------------------------------------------------------------

impl FrozenScopeGraph {
    // -- Name-environment delegation --

    /// Extend the name table after freeze (used by the resolver during fact
    /// building).  This does not change frozen scope/binding/mutation indexes.
    pub(in crate::analysis) fn intern_name_mut(&mut self, name: &str) -> Option<NameId> {
        self.names.intern_name_mut(name)
    }

    pub(in crate::analysis) fn name_table_mut(&mut self) -> &mut NameTable {
        self.names.name_table_mut()
    }

    pub(in crate::analysis) fn name_table_exhausted(&self) -> bool {
        self.names.name_table_exhausted()
    }

    pub(in crate::analysis) fn name_exhaustion(
        &self,
    ) -> Option<crate::analysis::name::NameExhausted> {
        self.names.name_exhaustion()
    }

    pub(in crate::analysis) fn into_name_table(self) -> NameTable {
        self.names.into_name_table()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn name_snapshot(&self) -> NameTable {
        self.names.name_snapshot()
    }

    pub(in crate::analysis) fn resolve_name_id(&self, name: NameId) -> Option<SmolStr> {
        self.names.resolve_name_id(name)
    }

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.names.name_id(name)
    }

    pub(in crate::analysis) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.name_path(path)
    }

    pub(in crate::analysis) fn symbol_path(&self, path: &NamePath) -> Option<SymbolPath> {
        self.names.symbol_path(path)
    }

    pub(in crate::analysis) fn is_global(&self, name: &str) -> bool {
        self.names.is_global(name)
    }

    pub(super) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.names.is_global_member(root, member)
    }

    pub(in crate::analysis) fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.names.global_objects()
    }

    // -- Lexical-scope-index delegation --

    pub(in crate::analysis) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.scope_parent(scope)
    }

    pub(super) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.scopes.scope_kind(scope)
    }

    pub(super) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.scopes.scope_span(scope)
    }

    pub(super) fn scope_binding(&self, scope: ScopeId, name: NameId) -> Option<&BindingProvenance> {
        self.scopes.scope_binding(scope, name)
    }

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> ScopeId {
        self.scopes.scope_at(span, true)
    }

    // -- Binding-index delegation --

    pub(super) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> Option<&AliasAssignment> {
        self.bindings.assignment_at(scope, name, span)
    }

    pub(super) fn binding_id_at(&self, scope: ScopeId, name: NameId) -> Option<BindingId> {
        self.bindings.binding_id_at(scope, name)
    }

    pub(super) fn parameter_alias_for(
        &self,
        function: FunctionId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.bindings.parameter_alias_for(function, name)
    }

    pub(super) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.bindings.reassigned_between(scope, name, start, end)
    }

    pub(super) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.bindings.binding_version(scope, name, span)
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.bindings.function_for_scope(scope)
    }

    pub(super) fn function_spans(&self) -> impl Iterator<Item = (FunctionId, Span)> + '_ {
        self.bindings.function_spans(&self.scopes)
    }

    pub(super) fn function_binding(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.bindings.function_binding(scope, name)
    }

    pub(super) fn function_alias(&self, scope: ScopeId, name: NameId) -> Option<FunctionId> {
        self.bindings.function_alias(scope, name)
    }

    // -- Mutation-index delegation --

    pub(super) fn property_aliases(
        &self,
        key: &(BindingKey, NamePath),
    ) -> Option<&[PropertyAliasFact]> {
        self.mutations.property_aliases(key)
    }

    pub(super) fn rooted_mutations(
        &self,
        root: &NamePath,
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.mutations.rooted_mutations(root)
    }

    pub(super) fn is_mutable_static_object(&self, scope: ScopeId, name: NameId) -> bool {
        self.mutations.is_mutable_static_object(scope, name)
    }

    pub(super) fn has_prior_eval(&self, scope: ScopeId, span: Span) -> bool {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if let Some(evals) = self.mutations.dynamic_evals_by_scope.get(&scope)
                && evals.partition_point(|effect| effect.span().hi < span.lo) > 0
            {
                return true;
            }
            current = self.scope_parent(scope);
        }
        false
    }
}

#[derive(Debug, Clone)]
/// One rooted property assignment indexed by receiver and path.
pub(in crate::analysis) struct PropertyAliasFact {
    pub(in crate::analysis::scope) span: Span,
    pub(in crate::analysis::scope) scope: ScopeId,
    pub(in crate::analysis::scope) target: Option<SymbolPath>,
}
