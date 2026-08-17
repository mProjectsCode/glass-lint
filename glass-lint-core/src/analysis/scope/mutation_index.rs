use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath, PathView};
use hashbrown::HashSet;
use swc_common::Span;

use crate::analysis::model::scope::{
    BindingKey, PropertyAliasFact, RootedPropertyMutationFact, ScopeEffect, ScopeId, ScopedName,
};

#[derive(Debug)]
pub(super) struct MutationIndexBuilder {
    property_assignments: BTreeMap<BindingKey, BTreeMap<NamePath, Vec<PropertyAliasFact>>>,
    rooted_property_mutations: BTreeMap<NamePath, Vec<RootedPropertyMutationFact>>,
    dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    mutable_static_objects: HashSet<ScopedName>,
}

#[derive(Debug)]
pub(super) struct MutationIndex {
    property_assignments: BTreeMap<BindingKey, BTreeMap<NamePath, Vec<PropertyAliasFact>>>,
    rooted_property_mutations: BTreeMap<NamePath, Vec<RootedPropertyMutationFact>>,
    dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    mutable_static_objects: HashSet<ScopedName>,
}

impl From<HashSet<ScopedName>> for MutationIndexBuilder {
    fn from(mutable_static_objects: HashSet<ScopedName>) -> Self {
        Self {
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            dynamic_evals_by_scope: BTreeMap::new(),
            mutable_static_objects,
        }
    }
}

impl MutationIndexBuilder {
    pub(super) fn record_property_assignment(
        &mut self,
        receiver: BindingKey,
        path: NamePath,
        fact: PropertyAliasFact,
    ) {
        self.property_assignments
            .entry(receiver)
            .or_default()
            .entry(path)
            .or_default()
            .push(fact);
    }

    pub(super) fn record_rooted_mutation(
        &mut self,
        root: NamePath,
        fact: RootedPropertyMutationFact,
    ) {
        self.rooted_property_mutations
            .entry(root)
            .or_default()
            .push(fact);
    }

    pub(super) fn record_dynamic_evals(
        &mut self,
        evals: impl IntoIterator<Item = (ScopeId, ScopeEffect)>,
    ) {
        for (scope, effect) in evals {
            self.dynamic_evals_by_scope
                .entry(scope)
                .or_default()
                .push(effect);
        }
    }

    pub(super) fn finish(self) -> MutationIndex {
        let mut index = MutationIndex {
            property_assignments: self.property_assignments,
            rooted_property_mutations: self.rooted_property_mutations,
            dynamic_evals_by_scope: self.dynamic_evals_by_scope,
            mutable_static_objects: self.mutable_static_objects,
        };
        index.sort();
        index
    }
}

impl MutationIndex {
    fn sort(&mut self) {
        for receiver_assignments in self.property_assignments.values_mut() {
            for assignments in receiver_assignments.values_mut() {
                assignments.sort_by_key(|assignment| assignment.span().lo);
            }
        }
        for mutations in self.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span().lo);
        }
        for spans in self.dynamic_evals_by_scope.values_mut() {
            spans.sort_by_key(|effect| effect.span().hi);
        }
    }

    fn property_aliases(
        &self,
        receiver: &BindingKey,
        path: PathView<'_, NameId>,
    ) -> Option<&[PropertyAliasFact]> {
        self.property_assignments
            .get(receiver)?
            .get(path.segments())
            .map(Vec::as_slice)
    }

    fn rooted_mutations(
        &self,
        root: PathView<'_, NameId>,
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.rooted_property_mutations
            .get(root.segments())
            .map(Vec::as_slice)
    }

    pub(super) fn latest_property_assignment_in_scope(
        &self,
        receiver: &BindingKey,
        path: PathView<'_, NameId>,
        span: Span,
        in_scope: impl Fn(ScopeId) -> bool,
    ) -> Option<&PropertyAliasFact> {
        let assignments = self.property_aliases(receiver, path)?;
        let prior_count = assignments.partition_point(|assignment| assignment.span().lo <= span.lo);
        assignments[..prior_count]
            .iter()
            .rev()
            .find(|assignment| in_scope(assignment.scope()))
    }

    pub(super) fn property_was_written_in_scope(
        &self,
        receiver: &BindingKey,
        path: PathView<'_, NameId>,
        span: Span,
        in_scope: impl Fn(ScopeId) -> bool,
    ) -> bool {
        self.property_aliases(receiver, path)
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span().lo <= span.lo && in_scope(assignment.scope())
                })
            })
    }

    pub(super) fn rooted_property_was_mutated_in_scope(
        &self,
        root: PathView<'_, NameId>,
        property: Option<NameId>,
        span: Span,
        in_scope: impl Fn(ScopeId) -> bool,
    ) -> bool {
        self.rooted_mutations(root).is_some_and(|mutations| {
            mutations.iter().any(|mutation| {
                mutation.span().lo <= span.lo
                    && mutation
                        .property()
                        .is_none_or(|written| property.is_none_or(|expected| written == expected))
                    && in_scope(mutation.scope())
            })
        })
    }

    pub(super) fn is_mutable_static_object(&self, scope: ScopeId, name: NameId) -> bool {
        self.mutable_static_objects
            .contains(&ScopedName::new(scope, name))
    }

    pub(super) fn has_prior_eval(&self, scope: ScopeId, span: Span) -> bool {
        self.dynamic_evals_by_scope
            .get(&scope)
            .is_some_and(|evals| evals.partition_point(|effect| effect.span().hi < span.lo) > 0)
    }
}
