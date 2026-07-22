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
    api::rule::matcher::{
        CallMatcher, ClassMatcher, ConstructorMatcher, InstanceMemberCallMatcher, MatcherFamilyMut,
        MatcherSet, MemberCallMatcher, MemberCallProvenance, MemberReadProvenance,
        ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, SymbolProvenance, normalize_flows,
        normalize_strings,
    },
    rules::MemberReadMatcher,
};

impl MatcherSet {
    /// Consume a matcher and return its canonical deterministic representation.
    pub(super) fn normalize(mut self) -> Self {
        for family in self.families_mut() {
            match family {
                MatcherFamilyMut::Calls(values) => normalize_calls(values),
                MatcherFamilyMut::MemberCalls(values) => normalize_member_calls(values),
                MatcherFamilyMut::MemberReads(values) => normalize_member_reads(values),
                MatcherFamilyMut::Imports(values) | MatcherFamilyMut::StringContains(values) => {
                    normalize_strings(values);
                }
                MatcherFamilyMut::PackageImports(values) => {
                    values.sort();
                    values.dedup();
                }
                MatcherFamilyMut::Classes(values) => ClassMatcher::normalize_all(values),
                MatcherFamilyMut::Constructors(values) => ConstructorMatcher::normalize_all(values),
                MatcherFamilyMut::Flows(values) => normalize_flows(values),
                MatcherFamilyMut::ReturnedMemberCalls(values) => {
                    ReturnedMemberCallMatcher::normalize_all(values);
                }
                MatcherFamilyMut::ReturnedMemberReads(values) => {
                    ReturnedMemberReadMatcher::normalize_all(values);
                }
                MatcherFamilyMut::InstanceMemberCalls(values) => {
                    InstanceMemberCallMatcher::normalize_all(values);
                }
            }
        }
        self
    }
}

fn normalize_calls(values: &mut Vec<CallMatcher>) {
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

fn normalize_member_calls(values: &mut Vec<MemberCallMatcher>) {
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

fn normalize_member_reads(values: &mut Vec<MemberReadMatcher>) {
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
