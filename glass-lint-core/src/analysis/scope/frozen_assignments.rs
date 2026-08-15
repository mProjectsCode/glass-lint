use glass_lint_datastructures::NameId;
use hashbrown::HashMap;
use swc_common::{BytePos, Span};

use crate::analysis::model::scope::{AliasAssignment, BindingProvenance, BindingVersion, ScopeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum BindingResolutionStatus {
    /// No lexical declaration or parameter is visible at the use position.
    Absent,
    /// One complete binding provenance is visible.
    Complete,
    /// Multiple complete joined alternatives are visible.
    Joined,
    /// At least one joined alternative was unknown or exhausted.
    Incomplete,
}

#[derive(Debug, Clone, Copy)]
enum BindingResolutionSource<'a> {
    None,
    Single(&'a BindingProvenance),
    Assignment(&'a AliasAssignment),
}

/// Borrowed binding witnesses together with the completeness of the lookup.
///
/// Complete witnesses remain available when an independent alternative is
/// incomplete. Callers must use [`status`](Self::status) for fallback and
/// certainty decisions instead of inferring them from witness count.
#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) struct BindingResolution<'a> {
    source: BindingResolutionSource<'a>,
    status: BindingResolutionStatus,
}

impl<'a> BindingResolution<'a> {
    pub(super) fn absent() -> Self {
        Self {
            source: BindingResolutionSource::None,
            status: BindingResolutionStatus::Absent,
        }
    }

    fn complete(witness: &'a BindingProvenance) -> Self {
        Self {
            source: BindingResolutionSource::Single(witness),
            status: BindingResolutionStatus::Complete,
        }
    }

    fn assignment(assignment: &'a AliasAssignment) -> Self {
        let status = if assignment.is_incomplete() {
            BindingResolutionStatus::Incomplete
        } else if assignment.is_joined() {
            BindingResolutionStatus::Joined
        } else {
            BindingResolutionStatus::Complete
        };
        Self {
            source: BindingResolutionSource::Assignment(assignment),
            status,
        }
    }

    /// Return the lookup status used for fallback and certainty decisions.
    pub(super) fn status(self) -> BindingResolutionStatus {
        self.status
    }

    /// Return the preferred retained witness, if one exists.
    pub(super) fn preferred_witness(self) -> Option<&'a BindingProvenance> {
        match self.source {
            BindingResolutionSource::None => None,
            BindingResolutionSource::Single(witness) => Some(witness),
            BindingResolutionSource::Assignment(assignment) => assignment.preferred_witness(),
        }
    }

    /// Visit each retained complete witness without exposing backing storage.
    pub(super) fn for_each_witness(self, mut visit: impl FnMut(&'a BindingProvenance)) {
        match self.source {
            BindingResolutionSource::None => {}
            BindingResolutionSource::Single(witness) => visit(witness),
            BindingResolutionSource::Assignment(assignment) => {
                assignment.complete_witnesses().for_each(visit);
            }
        }
    }
}

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
    /// Resolve an assignment together with its retained witnesses and status.
    pub(super) fn resolve(
        self,
        parameter: Option<&'a BindingProvenance>,
        declaration: &'a BindingProvenance,
    ) -> BindingResolution<'a> {
        match self {
            Self::Known(assignment) | Self::Ambiguous(assignment) => {
                BindingResolution::assignment(assignment)
            }
            Self::Absent => parameter.map_or_else(
                || BindingResolution::complete(declaration),
                BindingResolution::complete,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
/// Source-ordered assignment history frozen after collection.
///
/// All inner `Vec<AliasAssignment>` values are sorted by `span.lo`; this
/// invariant is established during construction and never violated.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct FrozenAssignmentIndex {
    inner: HashMap<ScopeId, HashMap<NameId, Vec<AliasAssignment>>>,
}

impl FrozenAssignmentIndex {
    /// Build from a flat, unsorted assignment stream.
    /// Sorts and groups by (scope, name) during construction.
    pub(in crate::analysis) fn from_assignments(assignments: Vec<AliasAssignment>) -> Self {
        let mut inner: HashMap<ScopeId, HashMap<NameId, Vec<AliasAssignment>>> = HashMap::new();
        for assignment in assignments {
            inner
                .entry(assignment.scope())
                .or_insert_with(HashMap::new)
                .entry(assignment.name())
                .or_default()
                .push(assignment);
        }
        for binding_assignments in inner.values_mut().flat_map(|m| m.values_mut()) {
            binding_assignments.sort_by_key(|a| a.span().lo);
        }
        Self { inner }
    }

    /// Retrieve the sorted slice for one scope/name pair, if it exists.
    fn get(&self, scope: ScopeId, name: NameId) -> Option<&[AliasAssignment]> {
        self.inner.get(&scope)?.get(&name).map(Vec::as_slice)
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
