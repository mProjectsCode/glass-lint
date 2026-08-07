use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::model::scope::{AliasAssignment, BindingProvenance, BindingVersion, ScopeId};

#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) enum AssignmentAt<'a> {
    Absent,
    /// A single known provenance.
    Known(&'a AliasAssignment),
    /// A synthetic post-join assignment with multiple provenance alternatives.
    /// The assignment carries all alternatives; at least one is non-Local.
    Ambiguous(&'a AliasAssignment),
}

impl<'a> AssignmentAt<'a> {
    /// Select the strict witness for an assignment or the declaration path
    /// when no assignment reaches the use position.
    pub(super) fn preferred_witness(
        self,
        parameter: Option<&'a BindingProvenance>,
        declaration: &'a BindingProvenance,
    ) -> Option<&'a BindingProvenance> {
        match self {
            Self::Known(assignment) | Self::Ambiguous(assignment) => assignment.preferred_witness(),
            Self::Absent => parameter.or(Some(declaration)),
        }
    }
}

/// Source-ordered assignment history frozen after collection.
///
/// All inner `Vec<AliasAssignment>` values are sorted by `span.lo`; this
/// invariant is established during construction and never violated.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct FrozenAssignmentIndex {
    inner: Vec<HashMap<NameId, Vec<AliasAssignment>>>,
}

impl FrozenAssignmentIndex {
    /// Build from a flat, unsorted assignment stream.
    /// Sorts and groups by (scope, name) during construction.
    pub(in crate::analysis) fn from_assignments(assignments: Vec<AliasAssignment>) -> Self {
        let max_scope = assignments
            .iter()
            .map(|a| a.scope().index())
            .max()
            .unwrap_or(0);
        let mut inner: Vec<HashMap<NameId, Vec<AliasAssignment>>> =
            vec![HashMap::new(); max_scope + 1];
        for assignment in assignments {
            inner[assignment.scope().index()]
                .entry(assignment.name())
                .or_default()
                .push(assignment);
        }
        for binding_assignments in inner.iter_mut().flat_map(|m| m.values_mut()) {
            binding_assignments.sort_by_key(|a| a.span().lo);
        }
        Self { inner }
    }

    /// Retrieve the sorted slice for one scope/name pair, if it exists.
    fn get(&self, scope: ScopeId, name: NameId) -> Option<&[AliasAssignment]> {
        self.inner.get(scope.index())?.get(&name).map(Vec::as_slice)
    }

    /// Find the index of the latest assignment at or before `span.lo`.
    fn latest_index(assignments: &[AliasAssignment], span: Span) -> Option<usize> {
        let idx = assignments.partition_point(|a| a.span().lo <= span.lo);
        idx.checked_sub(1)
    }

    /// Resolve the latest reaching definition at a source position.
    ///
    /// A conditional textual definition is explicitly ambiguous. Callers
    /// must not fall back to declaration or parameter provenance in that case:
    /// the conditional write may have replaced the older strict identity.
    /// When ambiguous, the assignment carries all provenance alternatives.
    pub(super) fn latest_at(&self, scope: ScopeId, name: NameId, span: Span) -> AssignmentAt<'_> {
        let Some(assignments) = self.get(scope, name) else {
            return AssignmentAt::Absent;
        };
        let Some(pos) = Self::latest_index(assignments, span) else {
            return AssignmentAt::Absent;
        };
        if assignments[pos].is_joined() {
            AssignmentAt::Ambiguous(&assignments[pos])
        } else {
            // A branch write is precise for a use inside that branch; only
            // the synthetic post-join assignment is ambiguous.
            AssignmentAt::Known(&assignments[pos])
        }
    }

    /// Binding version visible at a source position.
    ///
    /// Unlike [`latest_at`], this method uses the *raw* source-order
    /// latest assignment for version tracking, so that binding keys
    /// remain unique per textual assignment even when the provenance is
    /// uncertain.
    pub(super) fn version_at(&self, scope: ScopeId, name: NameId, span: Span) -> BindingVersion {
        let Some(assignments) = self.get(scope, name) else {
            return BindingVersion::new(0);
        };
        let Some(latest) = Self::latest_index(assignments, span) else {
            return BindingVersion::new(0);
        };
        assignments[latest].version()
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
        let after_start = assignments.partition_point(|a| a.span().lo <= start);
        after_start < assignments.len() && assignments[after_start].span().lo <= end
    }
}
