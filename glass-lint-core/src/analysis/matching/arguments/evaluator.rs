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
        value::{ValueId, ValueTable},
    },
    api::compiler::rule::{EventPredicate, IdentityConstraint, QueryConstraint},
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
        constraints: &[QueryConstraint],
        paths: &PreparedClausePaths,
    ) -> bool {
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
                self.check_constrained_args(constraints, args, unwrap.as_deref())
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
                self.constraints_match(constraints, args)
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

    fn constraints_match(&self, constraints: &[QueryConstraint], args: &[CallArgInfo]) -> bool {
        constraints.iter().all(|constraint| match constraint {
            QueryConstraint::Argument(argument) => {
                args.get(argument.index()).is_some_and(|value| {
                    argument.matcher().matches(
                        &self.argument_with_overlay(value),
                        self.names,
                        self.values,
                    )
                })
            }
        })
    }

    fn check_constrained_args(
        &self,
        constraints: &[QueryConstraint],
        args: &[CallArgInfo],
        unwrap: Option<&CallUnwrap>,
    ) -> bool {
        unwrap.map_or_else(
            || self.constraints_match(constraints, args),
            |unwrap| self.constraints_match(constraints, &unwrap.effective_args),
        )
    }
}
