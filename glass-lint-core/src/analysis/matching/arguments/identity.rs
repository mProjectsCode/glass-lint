use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{FactPayload, SemanticFact},
        syntax::{SymbolCallProvenance, SymbolMemberProvenance},
    },
    api::compiler::rule::IdentityConstraint,
};

pub(super) fn call_identity_matches(
    identity: &IdentityConstraint,
    call_provenance: &SymbolCallProvenance,
    callee_name: Option<&SmolStr>,
    syntactic_path: Option<&NamePath>,
    any_name_path: Option<&NamePath>,
) -> bool {
    match identity {
        IdentityConstraint::Any { name } => {
            callee_name.is_some_and(|found| *found == *name)
                || any_name_path
                    .zip(syntactic_path)
                    .is_some_and(|(name_path, chain)| name_path == chain)
        }
        IdentityConstraint::Global { name } => {
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

pub(super) fn member_identity_matches(
    identity: &IdentityConstraint,
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
    match identity {
        IdentityConstraint::Any { .. } => {
            syntactic_path.is_some_and(|chain| chain == member)
                || rooted_chain.is_some_and(|chain| chain == member)
        }
        IdentityConstraint::Rooted { .. } => {
            let Some(path) = rooted_path else {
                return false;
            };
            rooted_chain.is_some_and(|chain| chain == path && chain == member)
        }
        IdentityConstraint::ModuleNamespace { module } => namespace_member_matches(
            module_member.as_ref(),
            member,
            |found_module| found_module == module,
            names,
        ),
        IdentityConstraint::PackageModuleNamespace { module } => namespace_member_matches(
            module_member.as_ref(),
            member,
            |found_module| module.matches(found_module),
            names,
        ),
        _ => false,
    }
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
