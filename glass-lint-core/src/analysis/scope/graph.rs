use glass_lint_datastructures::{NameId, NamePath, NameTable, PathView, SymbolPath};
#[cfg(test)]
use hashbrown::HashSet;
use smol_str::SmolStr;
use swc_common::{BytePos, Span};

use crate::{
    Environment,
    analysis::{
        model::scope::{
            BindingId, BindingKey, BindingProvenance, BindingVersion, FunctionId, LexicalScopes,
            PropertyAliasFact, RootedPropertyMutationFact, ScopeEffect, ScopeId, ScopeKind,
        },
        scope::{
            binding_index::BindingIndex,
            build::{PropertyAliasAssignment, RootedPropertyMutation, ScopedDynamicEval},
            frozen_assignments::AssignmentAt,
            mutation_index::{MutationIndex, MutationIndexBuilder},
            name_env::NameEnvironment,
            scope_index::LexicalScopeIndex,
        },
    },
};

mod storage;

use storage::{ScopeData, ScopeReadView};

// ---------------------------------------------------------------------------
// ScopeGraph — mutable collection-phase struct
// ---------------------------------------------------------------------------

#[derive(Debug)]
/// Mutable scope graph used during the collection phase.
///
/// After calling [`finish_collected_properties`] and [`freeze`], callers
/// receive a read-only [`FrozenScopeGraph`] for all query operations.
pub(in crate::analysis) struct ScopeGraph {
    data: ScopeData<MutationIndexBuilder>,
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
    data: ScopeData<MutationIndex>,
    /// False when collection and planned scope shapes diverged.
    scope_shape_valid: bool,
}

pub(super) struct ScopeGraphInput {
    pub(super) environment: Environment,
    pub(super) names: NameTable,
    pub(super) scopes: LexicalScopes,
    pub(super) bindings: BindingIndex,
    pub(super) mutations: MutationIndexBuilder,
    pub(super) scope_shape_valid: bool,
}

impl ScopeGraph {
    fn read_view(&self) -> ScopeReadView<'_, MutationIndexBuilder> {
        ScopeReadView {
            data: &self.data,
            scope_shape_valid: self.scope_shape_valid,
        }
    }

    /// Create a minimally-initialized scope graph for test use.
    #[cfg(test)]
    pub(in crate::analysis) fn create_for_test(names: NameTable) -> Self {
        Self {
            data: ScopeData {
                names: NameEnvironment::new(names, Environment::default()),
                scopes: LexicalScopeIndex::from(LexicalScopes::new()),
                bindings: BindingIndex::empty(),
                mutations: MutationIndexBuilder::from(HashSet::new()),
            },
            scope_shape_valid: true,
        }
    }

    /// Assemble a validated scope graph from the collector's freeze output.
    pub(super) fn from_collected(input: ScopeGraphInput) -> Self {
        let ScopeGraphInput {
            environment,
            names,
            scopes,
            bindings,
            mutations,
            scope_shape_valid,
        } = input;
        Self {
            data: ScopeData {
                names: NameEnvironment::new(names, environment),
                scopes: LexicalScopeIndex::from(scopes),
                bindings,
                mutations,
            },
            scope_shape_valid,
        }
    }

    /// Freeze this scope graph into a read-only query graph.
    pub fn freeze(self) -> FrozenScopeGraph {
        let ScopeData {
            names,
            scopes,
            bindings,
            mutations,
        } = self.data;
        FrozenScopeGraph {
            data: ScopeData {
                names,
                scopes,
                bindings,
                mutations: mutations.finish(),
            },
            scope_shape_valid: self.scope_shape_valid,
        }
    }

    // -- Name-related helpers kept on ScopeGraph for collection --

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.data.names.name_id(name)
    }

    pub(in crate::analysis) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.data.names.name_path(path)
    }

    /// Convert collector-side property events into sorted query indexes.
    pub(in crate::analysis) fn finish_collected_properties(
        &mut self,
        property_assignments: Vec<PropertyAliasAssignment>,
        rooted_property_mutations: Vec<RootedPropertyMutation>,
        dynamic_evals: Vec<ScopedDynamicEval>,
    ) {
        for assignment in property_assignments {
            let span = assignment.span();
            let scope = assignment.scope();
            let property = assignment.property();
            let receiver = assignment.receiver();
            let Some(receiver_key) =
                self.binding_key_for_name(receiver.sym.as_ref(), receiver.span)
            else {
                continue;
            };
            let path = property
                .without_first_segment()
                .and_then(|path| self.name_path(&path))
                .unwrap_or_default();
            let target = assignment.take_target();
            self.data.mutations.record_property_assignment(
                receiver_key,
                path,
                PropertyAliasFact::new(span, scope, target),
            );
        }
        for mutation in rooted_property_mutations {
            let span = mutation.span();
            let scope = mutation.scope();
            let property = mutation.property();
            let receiver = mutation.receiver();
            self.data.mutations.record_rooted_mutation(
                receiver,
                RootedPropertyMutationFact::new(span, scope, property),
            );
        }
        let evals: Vec<(ScopeId, ScopeEffect)> = dynamic_evals
            .into_iter()
            .filter_map(|eval| {
                let scope = eval.scope();
                let effect = eval.take_effect();
                self.preferred_binding_witness_at("eval", effect.span())
                    .is_none()
                    .then_some((scope, effect))
            })
            .collect();
        self.data.mutations.record_dynamic_evals(evals);
    }

    // -- Query methods needed during collection (also on FrozenScopeGraph) --

    /// Resolve one strict binding provenance visible at a use position.
    /// Ambiguous joins are handled by callers that need to evaluate every
    /// alternative; this compatibility query returns the first non-local
    /// witness only.
    pub(super) fn preferred_binding_witness_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<&BindingProvenance> {
        let name = self.name_id(name)?;
        let view = self.read_view();
        let (scope, declaration) = view.nearest_binding_at(name, span)?;
        let parameter = view.parameter_alias_for_scope(scope, name);
        view.assignment_at(scope, name, span)
            .resolve(parameter, declaration)
            .preferred_witness()
    }

    /// Build a stable key for a name, using a global root when unbound.
    fn binding_key_for_name(&self, name: &str, span: Span) -> Option<BindingKey> {
        self.read_view().binding_key_for_name(name, span)
    }
}

// ---------------------------------------------------------------------------
// FrozenScopeGraph — all query methods delegate to the sub-structs
// ---------------------------------------------------------------------------

impl FrozenScopeGraph {
    fn read_view(&self) -> ScopeReadView<'_, MutationIndex> {
        ScopeReadView {
            data: &self.data,
            scope_shape_valid: self.scope_shape_valid,
        }
    }

    // -- Name-environment delegation --

    pub(in crate::analysis) fn name_snapshot(&self) -> NameTable {
        self.data.names.name_snapshot()
    }

    pub(in crate::analysis) fn resolve_name_id(&self, name: NameId) -> Option<SmolStr> {
        self.data.names.resolve_name_id(name)
    }

    pub(in crate::analysis) fn name_id(&self, name: &str) -> Option<NameId> {
        self.data.names.name_id(name)
    }

    pub(in crate::analysis) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.data.names.name_path(path)
    }

    pub(in crate::analysis) fn symbol_path(&self, path: &NamePath) -> Option<SymbolPath> {
        self.data.names.symbol_path(path)
    }

    pub(in crate::analysis) fn is_global(&self, name: &str) -> bool {
        self.data.names.is_global(name)
    }

    pub(in crate::analysis) fn is_global_member(&self, root: &str, member: &str) -> bool {
        self.data.names.is_global_member(root, member)
    }

    pub(in crate::analysis) fn global_objects(&self) -> impl Iterator<Item = &str> {
        self.data.names.global_objects()
    }

    // -- Lexical-scope-index delegation --

    pub(in crate::analysis) fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.read_view().scope_parent(scope)
    }

    pub(in crate::analysis) fn scope_kind(&self, scope: ScopeId) -> Option<ScopeKind> {
        self.read_view().scope_kind(scope)
    }

    pub(in crate::analysis) fn scope_span(&self, scope: ScopeId) -> Option<Span> {
        self.read_view().scope_span(scope)
    }

    pub(in crate::analysis) fn scope_at(&self, span: Span) -> Option<ScopeId> {
        self.read_view().scope_at(span)
    }

    pub(in crate::analysis) fn enclosing_function_at(&self, scope: ScopeId) -> FunctionId {
        self.read_view().enclosing_function_at(scope)
    }

    pub(in crate::analysis) fn nearest_binding_at(
        &self,
        name: NameId,
        span: Span,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        let view = self.read_view();
        view.nearest_binding_at(name, span)
    }

    pub(in crate::analysis) fn nearest_binding_from_scope(
        &self,
        name: NameId,
        scope: ScopeId,
    ) -> Option<(ScopeId, &BindingProvenance)> {
        self.data.binding_with_scope_at(name, scope)
    }

    pub(in crate::analysis) fn parameter_alias_for_scope(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<&BindingProvenance> {
        let view = self.read_view();
        view.parameter_alias_for_scope(scope, name)
    }

    // -- Binding-index delegation --

    pub(in crate::analysis) fn assignment_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> AssignmentAt<'_> {
        self.read_view().assignment_at(scope, name, span)
    }

    pub(in crate::analysis) fn binding_id_at(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<BindingId> {
        self.read_view().binding_id_at(scope, name)
    }

    pub(in crate::analysis) fn reassigned_between(
        &self,
        scope: ScopeId,
        name: NameId,
        start: BytePos,
        end: BytePos,
    ) -> bool {
        self.read_view().reassigned_between(scope, name, start, end)
    }

    pub(in crate::analysis) fn binding_version(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> BindingVersion {
        self.read_view().binding_version(scope, name, span)
    }

    /// Build a stable key for a name, using a global root when unbound.
    pub(in crate::analysis) fn binding_key_for_name(
        &self,
        name: &str,
        span: Span,
    ) -> Option<BindingKey> {
        self.read_view().binding_key_for_name(name, span)
    }

    pub(in crate::analysis) fn function_span(&self, function: FunctionId) -> Option<Span> {
        self.read_view().function_span(function)
    }

    pub(in crate::analysis) fn function_containing(&self, span: Span) -> Option<FunctionId> {
        self.read_view().function_containing(span)
    }

    pub(in crate::analysis) fn function_binding(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<FunctionId> {
        self.read_view().function_binding(scope, name)
    }

    pub(in crate::analysis) fn function_alias(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> Option<FunctionId> {
        self.read_view().function_alias(scope, name)
    }

    // -- Mutation-index delegation --

    pub(in crate::analysis) fn property_aliases(
        &self,
        receiver: &BindingKey,
        path: PathView<'_, NameId>,
    ) -> Option<&[PropertyAliasFact]> {
        self.data.mutations.property_aliases(receiver, path)
    }

    pub(in crate::analysis) fn rooted_mutations(
        &self,
        root: PathView<'_, NameId>,
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.data.mutations.rooted_mutations(root)
    }

    pub(in crate::analysis) fn is_mutable_static_object(
        &self,
        scope: ScopeId,
        name: NameId,
    ) -> bool {
        self.data.mutations.is_mutable_static_object(scope, name)
    }

    pub(in crate::analysis) fn has_prior_eval(&self, scope: ScopeId, span: Span) -> bool {
        let mut current = Some(scope);
        while let Some(scope) = current {
            if self.data.mutations.has_prior_eval(scope, span) {
                return true;
            }
            current = self.scope_parent(scope);
        }
        false
    }
}
