use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo, FactPayload, SemanticFact},
        matching::{
            MatcherProjectOverlay,
            arguments::identity::{call_identity_matches, member_identity_matches},
        },
        model::value::ValueTable,
    },
    api::compiler::{
        normalized::CanonicalArgumentConstraints,
        rule::{EventSpec, IdentityConstraint},
    },
};

pub(super) struct PreparedClausePaths {
    member: Option<NamePath>,
    rooted: Option<NamePath>,
    any_name: Option<NamePath>,
}

impl PreparedClausePaths {
    pub(super) fn new(identity: &IdentityConstraint, event: &EventSpec, names: &NameTable) -> Self {
        let member = match event {
            EventSpec::MemberCall { member }
            | EventSpec::MemberRead { member }
            | EventSpec::PropertyWrite { property: member } => names.lookup_path(member),
            _ => None,
        };
        let rooted = match identity {
            IdentityConstraint::Rooted { path } => names.lookup_path(path),
            _ => None,
        };
        let any_name = match identity {
            IdentityConstraint::Any { name } => names.lookup_path(&SymbolPath::from(name.as_str())),
            _ => None,
        };
        Self {
            member,
            rooted,
            any_name,
        }
    }
}

/// Operations charged during argument evaluation.
///
/// Tracks per-candidate, per-group, and per-predicate operations
/// for deterministic operation-count verification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EvaluationOperations {
    /// Number of candidates (facts) evaluated.
    pub(super) candidates: usize,
    /// Number of unique argument groups checked.
    pub(super) groups: usize,
    /// Number of individual predicate applications.
    pub(super) predicates: usize,
    /// Number of overlay-ready argument views constructed
    /// (one per unique group index per candidate).
    pub(super) argument_preparations: usize,
    /// Number of value-table resolutions performed while preparing groups.
    pub(super) value_resolutions: usize,
}

impl EvaluationOperations {
    pub(super) fn charge_candidate(&mut self) {
        self.candidates = self.candidates.saturating_add(1);
    }

    pub(super) fn charge_group(&mut self) {
        self.groups = self.groups.saturating_add(1);
    }

    pub(super) fn charge_predicate(&mut self) {
        self.predicates = self.predicates.saturating_add(1);
    }

    pub(super) fn charge_argument_preparation(&mut self) {
        self.argument_preparations = self.argument_preparations.saturating_add(1);
    }

    pub(super) fn charge_value_resolution(&mut self) {
        self.value_resolutions = self.value_resolutions.saturating_add(1);
    }
}

pub(super) struct MatcherEvaluator<'a> {
    names: &'a NameTable,
    values: &'a ValueTable,
    identity: MatcherProjectOverlay<'a>,
}

impl<'a> MatcherEvaluator<'a> {
    pub(super) fn new(
        names: &'a NameTable,
        values: &'a ValueTable,
        project: MatcherProjectOverlay<'a>,
    ) -> Self {
        Self {
            names,
            values,
            identity: project,
        }
    }

    pub(super) fn fact_matches_clause(
        &self,
        fact: &SemanticFact,
        identity: &IdentityConstraint,
        event: &EventSpec,
        constraints: &CanonicalArgumentConstraints,
        paths: &PreparedClausePaths,
        ops: &mut EvaluationOperations,
    ) -> bool {
        ops.charge_candidate();
        let FactPayload::Call(call) = fact.payload() else {
            return false;
        };
        let callee = call.callee();
        let callee_name: Option<SmolStr> = call
            .callee_name()
            .and_then(|id| self.names.resolve(id).map(Into::into));
        let call_provenance = self
            .identity
            .call_provenance(call.call_provenance(), callee);

        match event {
            EventSpec::Call => {
                if !call_identity_matches(
                    identity,
                    &call_provenance,
                    callee_name.as_ref(),
                    call.syntactic_path(),
                    paths.any_name.as_ref(),
                ) {
                    return false;
                }
                self.check_constrained_args(fact.payload(), constraints, ops)
            }
            EventSpec::MemberCall { .. } => {
                let Some(ref member) = paths.member else {
                    return false;
                };
                if !member_identity_matches(
                    identity,
                    member,
                    paths.rooted.as_ref(),
                    call.syntactic_path(),
                    call.rooted_chain(),
                    fact,
                    self.names,
                ) {
                    return false;
                }
                self.check_constrained_args(fact.payload(), constraints, ops)
            }
            _ => false,
        }
    }

    pub(super) fn argument_with_overlay<'b>(
        &'b self,
        argument: &'b CallArgInfo,
    ) -> ArgumentView<'b> {
        let (object, rooted_chain) = self.values.object_and_chain(argument.value);
        let value = self.values.resolve(argument.value);
        let static_string =
            self.identity
                .static_string(argument.value, &argument.provenance, value);
        ArgumentView::new(static_string, object, rooted_chain)
    }

    fn constraints_match(
        &self,
        constraints: &CanonicalArgumentConstraints,
        args: &[CallArgInfo],
        ops: &mut EvaluationOperations,
    ) -> bool {
        constraints.groups().iter().all(|group| {
            let idx = group.index().get();
            let Some(value) = args.get(idx) else {
                return false;
            };
            ops.charge_group();
            ops.charge_argument_preparation();
            ops.charge_value_resolution();
            let view = self.argument_with_overlay(value);
            group.predicates().iter().all(|matcher| {
                ops.charge_predicate();
                matcher.matches(&view, self.names, self.values)
            })
        })
    }

    fn check_constrained_args(
        &self,
        payload: &FactPayload,
        constraints: &CanonicalArgumentConstraints,
        ops: &mut EvaluationOperations,
    ) -> bool {
        let Some(effective) = payload.effective_call_args() else {
            return false;
        };
        self.constraints_match(constraints, effective, ops)
    }
}
