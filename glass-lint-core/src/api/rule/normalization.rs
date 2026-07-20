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
        StaticStringPredicate,
        matcher::{
            ArgumentConstraint, ArgumentMatcher, CallMatcher, ClassMatcher, ConstructorMatcher,
            InstanceMemberCallMatcher, MatcherFamilyMut, MatcherSet, MemberCallMatcher,
            MemberCallProvenance, MemberReadProvenance, ReturnedMemberCallMatcher,
            ReturnedMemberReadMatcher, SymbolProvenance, ValueMatcher, ValueMatcherKind,
            canonical_rooted_chain, normalize_flows, normalize_member_chain, normalize_strings,
        },
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
        !call.name.is_empty()
            && match &call.provenance {
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
        !call.chain.is_empty()
            && match &call.provenance {
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
        member_read.chain = normalize_member_chain(&member_read.chain);
        if member_read.provenance == MemberReadProvenance::Rooted {
            member_read.chain = canonical_rooted_chain(&member_read.chain).to_string();
        }
        if let MemberReadProvenance::ModuleNamespace { module } = &mut member_read.provenance {
            *module = module.trim().to_string();
        }
    }
    values.retain(|read| {
        !read.chain.is_empty()
            && match &read.provenance {
                MemberReadProvenance::Any
                | MemberReadProvenance::Rooted
                | MemberReadProvenance::PackageModuleNamespace { .. } => true,
                MemberReadProvenance::ModuleNamespace { module } => !module.is_empty(),
            }
    });
    values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    values.dedup();
}

impl ClassMatcher {
    fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provenance.normalize();
    }

    pub fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.name.is_empty());
        values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        values.dedup();
    }
}

impl ConstructorMatcher {
    fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provenance.normalize();
    }

    pub fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.name.is_empty());
        values.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        values.dedup();
    }
}

impl ReturnedMemberCallMatcher {
    fn normalize(&mut self) {
        self.source = canonical_rooted_chain(&normalize_member_chain(&self.source)).to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.source.is_empty() && !value.member.is_empty());
        values.sort_by(|left, right| {
            (&left.source, &left.member).cmp(&(&right.source, &right.member))
        });
        values.dedup();
    }
}

impl ReturnedMemberReadMatcher {
    fn normalize(&mut self) {
        self.source = canonical_rooted_chain(&normalize_member_chain(&self.source)).to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| !value.source.is_empty() && !value.member.is_empty());
        values.sort_by(|left, right| {
            (&left.source, &left.member).cmp(&(&right.source, &right.member))
        });
        values.dedup();
    }
}

impl InstanceMemberCallMatcher {
    fn normalize(&mut self) {
        self.module = self.module.trim().to_string();
        self.export = self.export.trim().to_string();
        self.member = self.member.trim().to_string();
    }

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
        for value in values.iter_mut() {
            value.normalize();
        }
        values.retain(|value| {
            !value.module.is_empty() && !value.export.is_empty() && !value.member.is_empty()
        });
        values.sort_by(|left, right| {
            (&left.module, &left.export, &left.member).cmp(&(
                &right.module,
                &right.export,
                &right.member,
            ))
        });
        values.dedup();
    }
}

impl SymbolProvenance {
    pub fn normalize(&mut self) {
        if let Self::ModuleExport { module } = self {
            *module = module.trim().to_string();
        }
    }
}

impl CallMatcher {
    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provenance.normalize();
        ArgumentConstraint::normalize_all(&mut self.arguments);
    }
}

impl MemberCallMatcher {
    pub(crate) fn normalize(&mut self) {
        self.chain = normalize_member_chain(&self.chain);
        if self.provenance == MemberCallProvenance::Rooted {
            self.chain = canonical_rooted_chain(&self.chain).to_string();
        }
        if let MemberCallProvenance::ModuleNamespace { module } = &mut self.provenance {
            *module = module.trim().to_string();
        }
        ArgumentConstraint::normalize_all(&mut self.arguments);
    }
}

impl ValueMatcher {
    pub(crate) fn normalize(&mut self) {
        if let ValueMatcherKind::StaticString(predicate) = &mut self.kind {
            match predicate {
                StaticStringPredicate::Any => {}
                StaticStringPredicate::Exact(values)
                | StaticStringPredicate::Prefix(values)
                | StaticStringPredicate::ContainsAny(values)
                | StaticStringPredicate::ContainsAll(values) => {
                    normalize_strings(values);
                }
            }
        }
    }
}

impl ArgumentMatcher {
    pub(crate) fn normalize(&mut self) {
        match self {
            Self::Value(value) => value.normalize(),
            Self::ObjectKeys(keys) | Self::RootedExpressions(keys) => normalize_strings(keys),
            Self::ObjectPropertyValue { property, value } => {
                *property = property.trim().to_string();
                value.normalize();
            }
        }
    }
}

impl ArgumentConstraint {
    pub fn normalize_all(arguments: &mut Vec<Self>) {
        for argument in arguments.iter_mut() {
            argument.matcher.normalize();
        }
        arguments.sort_by_key(|argument| argument.index);
        arguments.dedup();
    }
}
