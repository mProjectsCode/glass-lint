use std::collections::BTreeMap;

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo, FactPayload, SemanticFact},
        matching::{
            ModuleExportKey, ModuleIdentityMap,
            arguments::identity::{call_identity_matches, member_identity_matches},
        },
        model::value::{Value, ValueId, ValueTable},
        project::model::ExportResolution,
        syntax::SymbolCallProvenance,
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
    identity: EffectiveIdentityResolver<'a>,
}

/// Resolves one local value through the project overlay in its canonical
/// precedence order: call-result identity, module identity, then local value
/// data. Consumers choose the representation they need after this lookup.
struct EffectiveIdentityResolver<'a> {
    identities: Option<&'a ModuleIdentityMap>,
    result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
}

impl<'a> EffectiveIdentityResolver<'a> {
    fn new(
        identities: Option<&'a ModuleIdentityMap>,
        result_identities: Option<&'a BTreeMap<ValueId, ExportResolution>>,
    ) -> Self {
        Self {
            identities,
            result_identities,
        }
    }

    fn module_identity(&self, provenance: &SymbolCallProvenance) -> Option<&ExportResolution> {
        let (module, export) = provenance.module_export_parts()?;
        self.identities?.get(&ModuleExportKey::new(module, export))
    }

    fn result_identity(&self, value: ValueId) -> Option<&ExportResolution> {
        self.result_identities?.get(&value)
    }

    fn effective_identity(
        &self,
        value: ValueId,
        provenance: &SymbolCallProvenance,
    ) -> Option<&ExportResolution> {
        self.result_identity(value)
            .or_else(|| self.module_identity(provenance))
    }

    fn static_string<'b>(
        &'b self,
        value: ValueId,
        provenance: &SymbolCallProvenance,
        local_value: Option<&'b Value>,
    ) -> Option<&'b str> {
        self.effective_identity(value, provenance)
            .and_then(ExportResolution::static_string_value)
            .or_else(|| match local_value? {
                Value::StaticString(value) => Some(value.as_str()),
                _ => None,
            })
    }

    fn call_provenance(&self, raw: &SymbolCallProvenance, callee: ValueId) -> SymbolCallProvenance {
        self.effective_identity(callee, raw)
            .and_then(ExportResolution::to_call_provenance)
            .unwrap_or_else(|| raw.clone())
    }
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
            identity: EffectiveIdentityResolver::new(identities, result_identities),
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
        let FactPayload::Call {
            callee,
            syntactic_path,
            rooted_chain,
            call_provenance,
            callee_name,
            ..
        } = &fact.payload
        else {
            return false;
        };
        let callee_name: Option<SmolStr> =
            callee_name.and_then(|id| self.names.resolve(id).map(Into::into));
        let call_provenance = self.identity.call_provenance(call_provenance, *callee);

        match event {
            EventSpec::Call => {
                if !call_identity_matches(
                    identity,
                    &call_provenance,
                    callee_name.as_ref(),
                    syntactic_path.as_ref(),
                    paths.any_name.as_ref(),
                ) {
                    return false;
                }
                self.check_constrained_args(&fact.payload, constraints, ops)
            }
            EventSpec::MemberCall { .. } => {
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
                self.check_constrained_args(&fact.payload, constraints, ops)
            }
            _ => false,
        }
    }

    pub(super) fn argument_with_overlay<'b>(
        &'b self,
        argument: &'b CallArgInfo,
    ) -> ArgumentView<'b> {
        let mut view = ArgumentView::new(argument);
        let value = self.values.resolve(argument.value);
        let (object, rooted_chain) = match value {
            Some(Value::StaticObject(object)) => (Some(object), None),
            Some(Value::RootedMember { path }) => (None, Some(path)),
            _ => (None, None),
        };
        view = view
            .with_static_object(object)
            .with_rooted_chain(rooted_chain);
        if let Some(value) =
            self.identity
                .static_string(argument.value, &argument.provenance, value)
        {
            view = view.with_static_string(value);
        }
        view
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
