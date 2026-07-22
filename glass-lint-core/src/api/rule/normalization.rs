//! Normalization of matcher argument predicates.
//!
//! Argument fields have their own nested sets and ordering rules. Keeping
//! this pass separate from top-level matcher normalization prevents a matcher
//! from being sorted before its semantic argument shape is canonicalized.
//!
//! Normalization trims strings, canonicalizes rooted chains, sorts nested
//! alternatives, and removes duplicates. It preserves matcher meaning while
//! making compiled catalogs deterministic.

use crate::{
    api::rule::{
        ModuleSpecifierPattern,
        matcher::{
            CallMatcher, MemberCallMatcher, MemberCallProvenance, MemberReadProvenance,
            SymbolProvenance,
        },
    },
    rules::MemberReadMatcher,
};

pub(super) fn normalize_calls(values: &mut Vec<CallMatcher>) {
    for call in values.iter_mut() {
        call.normalize();
    }
    values.retain(|call| {
        !call.name().is_empty()
            && match call.provenance() {
                SymbolProvenance::Any
                | SymbolProvenance::Global
                | SymbolProvenance::PackageModuleExport { .. } => true,
                SymbolProvenance::ModuleExport { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

pub(super) fn normalize_member_calls(values: &mut Vec<MemberCallMatcher>) {
    for member_call in values.iter_mut() {
        member_call.normalize();
    }
    values.retain(|call| {
        !call.chain().is_empty()
            && match call.provenance() {
                MemberCallProvenance::Any
                | MemberCallProvenance::Rooted
                | MemberCallProvenance::PackageModuleNamespace { .. } => true,
                MemberCallProvenance::ModuleNamespace { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

pub(super) fn normalize_member_reads(values: &mut Vec<MemberReadMatcher>) {
    for member_read in values.iter_mut() {
        member_read.normalize();
    }
    values.retain(|read| {
        !read.chain().is_empty()
            && match read.provenance() {
                MemberReadProvenance::Any
                | MemberReadProvenance::Rooted
                | MemberReadProvenance::PackageModuleNamespace { .. } => true,
                MemberReadProvenance::ModuleNamespace { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

pub(super) fn normalize_package_imports(values: &mut Vec<ModuleSpecifierPattern>) {
    values.sort();
    values.dedup();
}
