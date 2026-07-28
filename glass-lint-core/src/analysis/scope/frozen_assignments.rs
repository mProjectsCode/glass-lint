use std::collections::BTreeMap;

use glass_lint_datastructures::NameId;
use swc_common::{BytePos, Span};

use crate::analysis::{
    model::scope::{AliasAssignment, ScopeId},
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

    /// Latest unconditional (definite) assignment at or before a source
    /// position.
    ///
    /// Returns `None` when the latest textual assignment is conditional,
    /// meaning the analysis cannot guarantee which definition reaches the
    /// use point.  Callers fall back to the declaration provenance, which
    /// conservatively preserves fail-closed identity.
    pub(super) fn latest_at(
        &self,
        scope: ScopeId,
        name: NameId,
        span: Span,
    ) -> Option<&AliasAssignment> {
        let assignments = self.get(scope, name)?;
        let pos = Self::latest_index(assignments, span)?;
        if !assignments[pos].conditional {
            return Some(&assignments[pos]);
        }
        // The latest assignment is conditional.  Scan backwards for the
        // latest unconditional assignment; if found, there is still a
        // conditional assignment more recent than it (the one we started
        // at) so the value is uncertain — return None.
        let mut i = pos;
        while i > 0 {
            i -= 1;
            if !assignments[i].conditional {
                return None;
            }
        }
        None
    }

    /// Binding version visible at a source position.
    ///
    /// Unlike [`latest_at`], this method uses the *raw* source-order
    /// latest assignment for version tracking, so that binding keys
    /// remain unique per textual assignment even when the provenance is
    /// uncertain.
    pub(super) fn version_at(&self, scope: ScopeId, name: NameId, span: Span) -> BindingVersion {
        let Some(assignments) = self.get(scope, name) else {
            return BindingVersion(0);
        };
        let Some(latest) = Self::latest_index(assignments, span) else {
            return BindingVersion(0);
        };
        assignments[latest].version
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
