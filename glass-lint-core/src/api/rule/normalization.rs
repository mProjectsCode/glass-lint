//! Normalization of matcher argument predicates.
//!
//! Argument fields have their own nested sets and ordering rules. Keeping
//! this pass separate from top-level matcher normalization prevents a matcher
//! from being sorted before its semantic argument shape is canonicalized.
//!
//! Normalization trims strings, canonicalizes rooted chains, sorts nested
//! alternatives, and removes duplicates. It preserves matcher meaning while
//! making compiled catalogs deterministic.

use super::matcher::{
    ArgumentConstraint, ArgumentMatcher, CallMatcher, ClassMatcher, ConstructorMatcher,
    InstanceMemberCallMatcher, MatcherSet, MemberCallMatcher, MemberCallProvenance,
    MemberReadProvenance, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, SymbolProvenance,
    ValueMatcher, ValueMatcherKind, canonical_rooted_chain, normalize_flows,
    normalize_member_chain, normalize_strings,
};

impl MatcherSet {
    /// Consume a matcher and return its canonical deterministic representation.
    pub(super) fn normalize(mut self) -> Self {
        normalize_arguments(&mut self);
        for call in &mut self.calls {
            call.normalize();
        }
        self.calls.retain(|call| {
            !call.name.is_empty()
                && match &call.provenance {
                    SymbolProvenance::Any | SymbolProvenance::Global => true,
                    SymbolProvenance::ModuleExport { module } => !module.is_empty(),
                }
        });
        self.calls
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.calls.dedup();

        for member_call in &mut self.member_calls {
            member_call.normalize();
        }
        self.member_calls.retain(|call| {
            !call.chain.is_empty()
                && match &call.provenance {
                    MemberCallProvenance::Any | MemberCallProvenance::Rooted => true,
                    MemberCallProvenance::ModuleNamespace { module } => !module.is_empty(),
                }
        });
        self.member_calls
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.member_calls.dedup();

        for member_read in &mut self.member_reads {
            member_read.chain = normalize_member_chain(&member_read.chain);
            if member_read.provenance == MemberReadProvenance::Rooted {
                member_read.chain = canonical_rooted_chain(&member_read.chain).to_string();
            }
            if let MemberReadProvenance::ModuleNamespace { module } = &mut member_read.provenance {
                *module = module.trim().to_string();
            }
        }
        self.member_reads.retain(|read| {
            !read.chain.is_empty()
                && match &read.provenance {
                    MemberReadProvenance::Any | MemberReadProvenance::Rooted => true,
                    MemberReadProvenance::ModuleNamespace { module } => !module.is_empty(),
                }
        });
        self.member_reads
            .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        self.member_reads.dedup();
        normalize_strings(&mut self.imports);
        normalize_strings(&mut self.string_contains);
        ClassMatcher::normalize_all(&mut self.classes);
        ConstructorMatcher::normalize_all(&mut self.constructors);
        normalize_flows(&mut self.flows);
        ReturnedMemberCallMatcher::normalize_all(&mut self.returned_member_calls);
        ReturnedMemberReadMatcher::normalize_all(&mut self.returned_member_reads);
        InstanceMemberCallMatcher::normalize_all(&mut self.instance_member_calls);
        self
    }
}

pub(super) fn normalize_arguments(matcher: &mut MatcherSet) {
    for call in &mut matcher.calls {
        ArgumentConstraint::normalize_all(&mut call.arguments);
    }

    for member_call in &mut matcher.member_calls {
        ArgumentConstraint::normalize_all(&mut member_call.arguments);
    }
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
                super::matcher::StaticStringPredicate::Any => {}
                super::matcher::StaticStringPredicate::Exact(values)
                | super::matcher::StaticStringPredicate::Prefix(values)
                | super::matcher::StaticStringPredicate::ContainsAny(values)
                | super::matcher::StaticStringPredicate::ContainsAll(values) => {
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
