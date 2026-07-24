use std::collections::BTreeMap;

use glass_lint_datastructures::NameId;
use swc_common::{BytePos, Span};

use crate::analysis::{
    scope::model::{id::ScopeId, types::AliasAssignment},
    value::BindingVersion,
};

/// Source-ordered assignment history frozen after collection.
///
/// All inner `Vec<AliasAssignment>` values are sorted by `span.lo`; this
/// invariant is established during construction and never violated.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct FrozenAssignmentIndex {
    inner: BTreeMap<ScopeId, BTreeMap<NameId, Vec<AliasAssignment>>>,
}

impl FrozenAssignmentIndex {
    /// Build from a flat, unsorted assignment stream.
    /// Sorts and groups by (scope, name) during construction.
    pub(in crate::analysis) fn from_assignments(assignments: Vec<AliasAssignment>) -> Self {
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
