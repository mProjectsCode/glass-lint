use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath};
use hashbrown::HashSet;
use swc_common::Span;

use crate::analysis::{
    model::scope::{
        PropertyAliasFact, RootedPropertyMutationFact, ScopeEffect, ScopeId, ScopedName,
    },
    value::BindingKey,
};

#[derive(Debug)]
pub(super) struct MutationIndex {
    property_assignments: BTreeMap<BindingKey, BTreeMap<NamePath, Vec<PropertyAliasFact>>>,
    rooted_property_mutations: BTreeMap<NamePath, Vec<RootedPropertyMutationFact>>,
    dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    mutable_static_objects: HashSet<ScopedName>,
}

impl From<HashSet<ScopedName>> for MutationIndex {
    fn from(mutable_static_objects: HashSet<ScopedName>) -> Self {
        Self {
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            dynamic_evals_by_scope: BTreeMap::new(),
            mutable_static_objects,
        }
    }
}

impl MutationIndex {
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
        self.dynamic_evals_by_scope.clear();
        for (scope, effect) in evals {
            self.dynamic_evals_by_scope
                .entry(scope)
                .or_default()
                .push(effect);
        }
    }

    pub(super) fn finalize(&mut self) {
        for receiver_assignments in self.property_assignments.values_mut() {
            for assignments in receiver_assignments.values_mut() {
                assignments.sort_by_key(|assignment| assignment.span.lo);
            }
        }
        for mutations in self.rooted_property_mutations.values_mut() {
            mutations.sort_by_key(|mutation| mutation.span.lo);
        }
        for spans in self.dynamic_evals_by_scope.values_mut() {
            spans.sort_by_key(|effect| effect.span().hi);
        }
    }

    pub(super) fn property_aliases(
        &self,
        receiver: &BindingKey,
        path: &[NameId],
    ) -> Option<&[PropertyAliasFact]> {
        self.property_assignments
            .get(receiver)?
            .get(path)
            .map(Vec::as_slice)
    }

    pub(super) fn rooted_mutations(
        &self,
        root: &[NameId],
    ) -> Option<&[RootedPropertyMutationFact]> {
        self.rooted_property_mutations.get(root).map(Vec::as_slice)
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
