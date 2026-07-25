use std::collections::BTreeMap;

use glass_lint_datastructures::{NameId, NamePath};
use hashbrown::HashSet;

use crate::analysis::{
    model::scope::{
        PropertyAliasFact, RootedPropertyMutationFact, ScopeEffect, ScopeId, ScopedName,
    },
    value::BindingKey,
};

#[derive(Debug)]
pub(super) struct MutationIndex {
    pub(super) property_assignments:
        BTreeMap<BindingKey, BTreeMap<NamePath, Vec<PropertyAliasFact>>>,
    pub(super) rooted_property_mutations: BTreeMap<NamePath, Vec<RootedPropertyMutationFact>>,
    pub(super) dynamic_evals_by_scope: BTreeMap<ScopeId, Vec<ScopeEffect>>,
    pub(super) mutable_static_objects: HashSet<ScopedName>,
}

impl MutationIndex {
    pub(super) fn new(mutable_static_objects: HashSet<ScopedName>) -> Self {
        Self {
            property_assignments: BTreeMap::new(),
            rooted_property_mutations: BTreeMap::new(),
            dynamic_evals_by_scope: BTreeMap::new(),
            mutable_static_objects,
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
}
