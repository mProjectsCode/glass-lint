use glass_lint_datastructures::{NameId, NamePath, NameTable, SymbolPath};
#[cfg(test)]
use hashbrown::{HashMap, HashSet};
use smol_str::SmolStr;
use swc_common::{BytePos, Span};

#[cfg(test)]
use crate::analysis::scope::FrozenAssignmentIndex;
use crate::{
    Environment,
    analysis::{
        model::scope::{
            BindingProvenance, LexicalScope, PropertyAliasFact, RootedPropertyMutationFact,
            ScopeEffect, ScopeId, ScopeKind,
        },
        scope::{
            binding_index::{BindingIndex, BindingIndexParts},
            build::{PropertyAliasAssignment, RootedPropertyMutation, ScopedDynamicEval},
            frozen_assignments::AssignmentAt,
            mutation_index::{MutationIndex, MutationIndexParts},
            name_env::NameEnvironment,
            scope_index::LexicalScopeIndex,
        },
        value::{BindingId, BindingKey, BindingRoot, BindingVersion, FunctionId},
    },
};

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
}

impl ScopeGraph {
    /// Create a minimally-initialized scope graph for test use.
    #[cfg(test)]
    pub(in crate::analysis) fn create_for_test(names: NameTable) -> Self {
        Self {
            names: NameEnvironment::new(names, Environment::default()),
            scopes: LexicalScopeIndex::new(Vec::new(), Vec::new()),
            bindings: BindingIndex::from_parts(BindingIndexParts {
                assignments: FrozenAssignmentIndex::from_assignments(Vec::new()),
                binding_ids: HashMap::new(),
                function_ids: Vec::new(),
                function_bindings: HashMap::new(),
                function_aliases: HashMap::new(),
                parameter_aliases: HashMap::new(),
            }),
            mutations: MutationIndex::from_parts(MutationIndexParts {
                mutable_static_objects: HashSet::new(),
            }),
            scope_shape_valid: true,
        }
    }

    /// Assemble the immutable graph before property indexes are attached.
    pub(in crate::analysis) fn from_parts(parts: ScopeGraphParts) -> Self {
        Self {
            names: NameEnvironment::new(parts.lexical.names, parts.environment),
            scopes: LexicalScopeIndex::new(parts.lexical.scopes, parts.lexical.scopes_by_start),
            bindings: BindingIndex::from_parts(parts.bindings),
            mutations: MutationIndex::from_parts(parts.mutations),
            scope_shape_valid: parts.scope_shape_valid,
        }
    }

    /// Freeze this scope graph into a read-only query graph.
    pub fn freeze(self) -> FrozenScopeGraph {
        FrozenScopeGraph {
            names: self.names,
            scopes: self.scopes,
            bindings: self.bindings,
            mutations: self.mutations,
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

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> ScopeId {
        self.scopes.scope_at(span, self.scope_shape_valid)
    }

    // -- Binding helpers on ScopeGraph --

    pub(super) fn assignment_at(&self, scope: ScopeId, name: &str, span: Span) -> AssignmentAt<'_> {
        let Some(name) = self.name_id(name) else {
            return AssignmentAt::Absent;
        };
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
            return BindingVersion::new(0);
        };
        self.bindings.binding_version(scope, name, span)
    }

    pub(super) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.bindings.function_for_scope(scope)
    }

    /// Convert collector-side property events into sorted query indexes.
    pub(in crate::analysis) fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<PropertyAliasAssignment>,
        rooted_mutations: Vec<RootedPropertyMutation>,
        dynamic_evals: Vec<ScopedDynamicEval>,
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
            self.mutations.record_property_assignment(
                receiver_key,
                path,
                PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target,
                },
            );
        }
        for mutation in rooted_mutations {
            self.mutations.record_rooted_mutation(
                mutation.receiver,
                RootedPropertyMutationFact {
                    span: mutation.span,
                    scope: mutation.scope,
                    property: mutation.property,
                },
            );
        }
        let evals: Vec<(ScopeId, ScopeEffect)> = dynamic_evals
            .into_iter()
            .filter(|eval| self.binding_at("eval", eval.effect.span()).is_none())
            .map(|eval| (eval.scope, eval.effect))
            .collect();
        self.mutations.record_dynamic_evals(evals);
        self.mutations.finalize();
    }

    // -- Query methods needed during collection (also on FrozenScopeGraph) --

    /// Resolve one strict binding provenance visible at a use position.
    /// Ambiguous joins are handled by callers that need to evaluate every
    /// alternative; this compatibility query returns the first non-local
    /// witness only.
    pub(super) fn binding_at(&self, name: &str, span: Span) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        match self.assignment_at(scope, name, span) {
            AssignmentAt::Known(assignment) => assignment.alternatives.first(),
            AssignmentAt::Ambiguous(assignment) => assignment
                .alternatives
                .iter()
                .find(|p| !matches!(p, BindingProvenance::Local)),
            AssignmentAt::Absent => self.parameter_alias_for(scope, name).or(Some(declaration)),
        }
    }

    /// Find the nearest lexical declaration and its owning scope.
    fn binding_with_scope_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        let name_id = self.name_id(name)?;
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scopes.scope_binding(scope, name_id) {
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
        FunctionId::new(0)
    }
}

/// Owned inputs used to assemble a collected [`ScopeGraph`].
pub(in crate::analysis) struct LexicalScopeParts {
    pub(in crate::analysis) names: NameTable,
    pub(in crate::analysis) scopes: Vec<LexicalScope>,
    pub(in crate::analysis) scopes_by_start: Vec<ScopeId>,
}

pub(in crate::analysis) struct ScopeGraphParts {
    pub(in crate::analysis) environment: Environment,
    pub(in crate::analysis) lexical: LexicalScopeParts,
    pub(in crate::analysis) bindings: BindingIndexParts,
    pub(in crate::analysis) mutations: MutationIndexParts,
    pub(in crate::analysis) scope_shape_valid: bool,
}

// ---------------------------------------------------------------------------
// FrozenScopeGraph — all query methods delegate to the sub-structs
// ---------------------------------------------------------------------------

impl FrozenScopeGraph {
    // -- Name-environment delegation --

    pub(in crate::analysis) fn name_table_mut(&mut self) -> &mut NameTable {
        self.names.name_table_mut()
    }

    pub(in crate::analysis) fn name_table_exhausted(&self) -> bool {
        self.names.name_table_exhausted()
    }

    pub(in crate::analysis) fn name_exhaustion(
        &self,
    ) -> Option<glass_lint_datastructures::NameExhausted> {
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

    pub(in crate::analysis) fn name_id(&self, name: &str) -> Option<NameId> {
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

    pub(in crate::analysis) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.names.is_global_member(root, member)
    }

    pub(in crate::analysis) fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.names.global_objects()
    }

    // -- Lexical-scope-index delegation --

    pub(in crate::analysis) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes.scope_parent(scope)
    }

    pub(in crate::analysis) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.scopes.scope_kind(scope)
    }

    pub(in crate::analysis) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.scopes.scope_span(scope)
    }

    pub(in crate::analysis) fn scope_binding(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.scopes.scope_binding(scope, name)
    }

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> ScopeId {
        self.scopes.scope_at(span, true)
    }

    // -- Binding-index delegation --

    pub(in crate::analysis) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> AssignmentAt<'_> {
        self.bindings.assignment_at(scope, name, span)
    }

    pub(in crate::analysis) fn binding_id_at(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<BindingId> {
        self.bindings.binding_id_at(scope, name)
    }

    pub(in crate::analysis) fn parameter_alias_for(
        &self,
        function: FunctionId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        self.bindings.parameter_alias_for(function, name)
    }

    pub(in crate::analysis) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.bindings.reassigned_between(scope, name, start, end)
    }

    pub(in crate::analysis) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.bindings.binding_version(scope, name, span)
    }

    pub(in crate::analysis) fn function_for_scope(&self, scope: ScopeId) -> Option<FunctionId> {
        self.bindings.function_for_scope(scope)
    }

    pub(in crate::analysis) fn function_spans(
        &self,
    ) -> impl Iterator<Item = (FunctionId, Span)> + '_ {
        self.bindings.function_spans(&self.scopes)
    }

    pub(in crate::analysis) fn function_binding(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<FunctionId> {
        self.bindings.function_binding(scope, name)
    }

    pub(in crate::analysis) fn function_alias(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<FunctionId> {
        self.bindings.function_alias(scope, name)
    }

    // -- Mutation-index delegation --

    pub(in crate::analysis) fn property_aliases(
        &self,
        receiver: &BindingKey,
        path: &[NameId],
    ) -> Option<&[PropertyAliasFact]> {
        self.mutations.property_aliases(receiver, path)
    }

    pub(in crate::analysis) fn rooted_mutations(
        &self,
        root: &[NameId],
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.mutations.rooted_mutations(root)
    }

    pub(in crate::analysis) fn is_mutable_static_object(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> bool {
        self.mutations.is_mutable_static_object(scope, name)
    }

    pub(in crate::analysis) fn has_prior_eval(&self, scope: ScopeId, span: Span) -> bool {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if self.mutations.has_prior_eval(scope, span) {
                return true;
            }
            current = self.scope_parent(scope);
        }
        false
    }
}
