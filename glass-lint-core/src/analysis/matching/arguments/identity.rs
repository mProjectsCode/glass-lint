use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{FactPayload, SemanticFact},
        syntax::{SymbolCallProvenance, SymbolMemberProvenance},
    },
    api::compiler::rule::{IdentityConstraint, QueryClause, SubjectConstraint},
};

pub(super) fn call_identity_matches(
    clause: &QueryClause,
    call_provenance: &SymbolCallProvenance,
    callee_name: Option<&SmolStr>,
    syntactic_path: Option<&NamePath>,
    any_name_path: Option<&NamePath>,
) -> bool {
    match &clause.identity {
        IdentityConstraint::Any { name, .. } => {
            callee_name.is_some_and(|found| *found == *name)
                || any_name_path
                    .zip(syntactic_path)
                    .is_some_and(|(name_path, chain)| name_path == chain)
        }
        IdentityConstraint::Global { name, .. } => {
            matches!(call_provenance, SymbolCallProvenance::Global { name: found } if found == name)
        }
        IdentityConstraint::ModuleExport { module, export } => {
            matches!(call_provenance, SymbolCallProvenance::ModuleExport {
                module: found_module, export: found_export
            } if found_module == module && found_export == export)
        }
        IdentityConstraint::PackageModuleExport { module, export } => {
            matches!(call_provenance, SymbolCallProvenance::ModuleExport {
                module: found_module, export: found_export
            } if module.matches(found_module) && found_export == export)
        }
        _ => false,
    }
}

pub(super) fn member_subject_matches(
    clause: &QueryClause,
    member: &NamePath,
    returned_member: Option<&(NamePath, NamePath)>,
    instance_class: Option<&(SmolStr, SmolStr)>,
    names: &NameTable,
) -> bool {
    match &clause.subject {
        SubjectConstraint::Direct => true,
        SubjectConstraint::ReturnedFrom { producer } => {
            returned_member.is_some_and(|(source, found)| {
                found == member
                    && names
                        .resolve_path(source)
                        .is_some_and(|source| producer.exact_root_matches(&source))
            })
        }
        SubjectConstraint::InstanceOf { constructor } => instance_class
            .is_some_and(|(module, export)| constructor.identity_module_matches(module, export)),
    }
}

pub(super) fn member_identity_matches(
    clause: &QueryClause,
    member: &NamePath,
    rooted_path: Option<&NamePath>,
    syntactic_path: Option<&NamePath>,
    rooted_chain: Option<&NamePath>,
    fact: &SemanticFact,
    names: &NameTable,
) -> bool {
    let FactPayload::Call { module_member, .. } = &fact.payload else {
        return false;
    };
    match (&clause.identity, &clause.subject) {
        (IdentityConstraint::Any { .. }, SubjectConstraint::Direct) => {
            syntactic_path.is_some_and(|chain| chain == member)
                || rooted_chain.is_some_and(|chain| chain == member)
        }
        (IdentityConstraint::Rooted { .. }, SubjectConstraint::Direct) => {
            let Some(path) = rooted_path else {
                return false;
            };
            rooted_chain.is_some_and(|chain| chain == path && chain == member)
        }
        (IdentityConstraint::Rooted { .. }, SubjectConstraint::ReturnedFrom { .. }) => {
            let FactPayload::Call {
                returned_member, ..
            } = &fact.payload
            else {
                return false;
            };
            let Some(path) = rooted_path else {
                return false;
            };
            returned_member
                .as_ref()
                .is_some_and(|(source, found)| source == path && found == member)
        }
        (
            IdentityConstraint::ModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => instance_class_and_chain_match(
            fact,
            syntactic_path,
            member,
            |found_module| found_module == module,
            export,
        ),
        (
            IdentityConstraint::PackageModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => instance_class_and_chain_match(
            fact,
            syntactic_path,
            member,
            |found_module| module.matches(found_module),
            export,
        ),
        (IdentityConstraint::ModuleNamespace { module }, SubjectConstraint::Direct) => {
            namespace_member_matches(
                module_member.as_ref(),
                member,
                |found_module| found_module == module,
                names,
            )
        }
        (IdentityConstraint::PackageModuleNamespace { module }, SubjectConstraint::Direct) => {
            namespace_member_matches(
                module_member.as_ref(),
                member,
                |found_module| module.matches(found_module),
                names,
            )
        }
        _ => false,
    }
}

fn instance_class_and_chain_match(
    fact: &SemanticFact,
    syntactic_path: Option<&NamePath>,
    member: &NamePath,
    module_matches: impl FnOnce(&SmolStr) -> bool,
    export: &SmolStr,
) -> bool {
    let FactPayload::Call { instance_class, .. } = &fact.payload else {
        return false;
    };
    instance_class
        .as_ref()
        .is_some_and(|(found_module, found_export)| {
            module_matches(found_module) && found_export == export
        })
        && syntactic_path
            .and_then(NamePath::last_segment)
            .zip(member.last_segment())
            .is_some_and(|(s_last, m_last)| s_last == m_last)
}

fn namespace_member_matches(
    module_member: Option<&SymbolMemberProvenance>,
    member: &NamePath,
    module_matches: impl FnOnce(&SmolStr) -> bool,
    names: &NameTable,
) -> bool {
    matches!(
        module_member,
        Some(SymbolMemberProvenance::ModuleNamespace {
            module: found_module, member: found_member
        }) if module_matches(found_module)
                && member
                    .first_segment()
                    .copied()
                    .and_then(|id| names.resolve(id))
                    .is_some_and(|resolved| resolved == found_member.as_str())
    )
}
