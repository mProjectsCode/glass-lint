use std::collections::BTreeMap;

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo, CallUnwrap, FactPayload, SemanticFact},
        matching::{
            ModuleIdentityMap,
            arguments::identity::{call_identity_matches, member_identity_matches},
        },
        project::model::ExportResolution,
        syntax::SymbolCallProvenance,
        value::{Value, ValueId, ValueTable},
    },
    api::compiler::{
        normalized::CanonicalArgumentConstraints,
        rule::{EventPredicate, IdentityConstraint},
    },
};

pub(super) struct PreparedClausePaths {
    member: Option<NamePath>,
    rooted: Option<NamePath>,
    any_name: Option<NamePath>,
}

impl PreparedClausePaths {
    pub(super) fn new(
        identity: &IdentityConstraint,
        event: &EventPredicate,
        names: &NameTable,
    ) -> Self {
        let member = match event {
            EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member } => {
                names.lookup_path(member)
            }
            _ => None,
        };
        let rooted = match identity {
            IdentityConstraint::Rooted { path } => names.lookup_path(path),
            _ => None,
        };
        let any_name = match identity {
            IdentityConstraint::Any { name, .. } => {
                names.lookup_path(&SymbolPath::from(name.as_str()))
            }
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
    identities: Option<&'a ModuleIdentityMap>,
    result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
}

impl<'a> MatcherEvaluator<'a> {
    pub(super) fn new(
        names: &'a NameTable,
        values: &'a ValueTable,
        identities: Option<&'a ModuleIdentityMap>,
        result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
    ) -> Self {
        Self {
            names,
            values,
            identities,
            result_identities,
        }
    }

    pub(super) fn fact_matches_clause(
        &self,
        fact: &SemanticFact,
        identity: &IdentityConstraint,
        event: &EventPredicate,
        constraints: &CanonicalArgumentConstraints,
        paths: &PreparedClausePaths,
        ops: &mut EvaluationOperations,
    ) -> bool {
        ops.charge_candidate();
        let FactPayload::Call {
            callee,
            syntactic_path,
            rooted_chain,
            call_provenance,
            callee_name,
            args,
            unwrap,
            ..
        } = &fact.payload
        else {
            return false;
        };
        let callee_name: Option<SmolStr> =
            callee_name.and_then(|id| self.names.resolve(id).map(Into::into));
        let call_provenance = self.overlaid_call_provenance(call_provenance, *callee);

        match event {
            EventPredicate::Call => {
                if !call_identity_matches(
                    identity,
                    &call_provenance,
                    callee_name.as_ref(),
                    syntactic_path.as_ref(),
                    paths.any_name.as_ref(),
                ) {
                    return false;
                }
                self.check_constrained_args(constraints, args, unwrap.as_deref(), ops)
            }
            EventPredicate::MemberCall { .. } => {
                let Some(ref member) = paths.member else {
                    return false;
                };
                if !member_identity_matches(
                    identity,
                    member,
                    paths.rooted.as_ref(),
                    syntactic_path.as_ref(),
                    rooted_chain.as_ref(),
                    fact,
                    self.names,
                ) {
                    return false;
                }
                self.constraints_match(constraints, args, ops)
            }
            _ => false,
        }
    }

    fn lookup_identity(&self, provenance: &SymbolCallProvenance) -> Option<&ExportResolution> {
        let (module, export) = provenance.module_export_parts()?;
        self.identities?.get_parts(module, export)
    }

    pub(super) fn argument_with_overlay<'b>(
        &'b self,
        argument: &'b CallArgInfo,
    ) -> ArgumentView<'b> {
        let mut view = ArgumentView::new(argument);
        let (object_entries, rooted_chain) = match self.values.resolve(argument.value) {
            Some(Value::StaticObject(entries)) => (Some(entries.as_slice()), None),
            Some(Value::RootedMember { path }) => (None, Some(path)),
            _ => (None, None),
        };
        view = view
            .with_object_entries(object_entries)
            .with_rooted_chain(rooted_chain);
        if let Some(result_identities) = self.result_identities
            && let Some(value) = result_identities
                .get(&argument.value)
                .and_then(ExportResolution::static_string_value)
        {
            view = view.with_static_string(value);
        }
        if let Some(identity) = self.lookup_identity(&argument.provenance)
            && let Some(value) = identity.static_string_value()
        {
            view = view.with_static_string(value);
        }
        if view.static_string.is_none()
            && let Some(value) = self.values.static_string(argument.value)
        {
            view = view.with_static_string(value);
        }
        view
    }

    fn overlaid_call_provenance(
        &self,
        raw: &SymbolCallProvenance,
        callee: ValueId,
    ) -> SymbolCallProvenance {
        if let Some(result_identities) = self.result_identities
            && let Some(identity) = result_identities.get(&callee)
            && let Some(provenance) = identity.to_call_provenance()
        {
            return provenance;
        }
        if let Some(identity) = self.lookup_identity(raw)
            && let Some(provenance) = identity.to_call_provenance()
        {
            return provenance;
        }
        raw.clone()
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
        constraints: &CanonicalArgumentConstraints,
        args: &[CallArgInfo],
        unwrap: Option<&CallUnwrap>,
        ops: &mut EvaluationOperations,
    ) -> bool {
        let effective = unwrap.map_or(args, |u| &u.effective_args);
        self.constraints_match(constraints, effective, ops)
    }
}
