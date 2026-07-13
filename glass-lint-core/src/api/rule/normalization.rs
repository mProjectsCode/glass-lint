//! Normalization of matcher argument predicates.
//!
//! Argument fields have their own nested sets and ordering rules. Keeping
//! this pass separate from top-level matcher normalization prevents a matcher
//! from being sorted before its semantic argument shape is canonicalized.

use super::matcher::{
    ApiMatcher, ArgumentConstraint, ArgumentMatcher, CallMatcher, CallProvenance, ClassMatcher,
    ConstructorMatcher, InstanceMemberCallMatcher, MemberCallMatcher, MemberCallProvenance,
    MemberReadProvenance, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, ValueMatcher,
    ValueMatcherKind, canonical_rooted_chain, normalize_flows, normalize_member_chain,
    normalize_strings,
};

pub(super) fn normalize(mut matcher: ApiMatcher) -> ApiMatcher {
    normalize_arguments(&mut matcher);
    for call in &mut matcher.calls {
        call.normalize();
    }
    matcher.calls.retain(|call| {
        !call.name.is_empty()
            && match &call.provenance {
                CallProvenance::Any | CallProvenance::Global => true,
                CallProvenance::ModuleExport { module } => !module.is_empty(),
            }
    });
    matcher
        .calls
        .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    matcher.calls.dedup();

    for member_call in &mut matcher.member_calls {
        member_call.normalize();
    }
    matcher.member_calls.retain(|call| {
        !call.chain.is_empty()
            && match &call.provenance {
                MemberCallProvenance::Any | MemberCallProvenance::Rooted => true,
                MemberCallProvenance::ModuleNamespace { module } => !module.is_empty(),
            }
    });
    matcher
        .member_calls
        .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    matcher.member_calls.dedup();

    for member_read in &mut matcher.member_reads {
        member_read.chain = normalize_member_chain(&member_read.chain);
        if member_read.provenance == MemberReadProvenance::Rooted {
            member_read.chain = canonical_rooted_chain(&member_read.chain).to_string();
        }
        if let MemberReadProvenance::ModuleNamespace { module } = &mut member_read.provenance {
            *module = module.trim().to_string();
        }
    }
    matcher.member_reads.retain(|read| {
        !read.chain.is_empty()
            && match &read.provenance {
                MemberReadProvenance::Any | MemberReadProvenance::Rooted => true,
                MemberReadProvenance::ModuleNamespace { module } => !module.is_empty(),
            }
    });
    matcher
        .member_reads
        .sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    matcher.member_reads.dedup();
    normalize_strings(&mut matcher.imports);
    normalize_strings(&mut matcher.string_literals);
    ClassMatcher::normalize_all(&mut matcher.classes);
    ConstructorMatcher::normalize_all(&mut matcher.constructors);
    normalize_flows(&mut matcher.flows);
    ReturnedMemberCallMatcher::normalize_all(&mut matcher.returned_member_calls);
    ReturnedMemberReadMatcher::normalize_all(&mut matcher.returned_member_reads);
    InstanceMemberCallMatcher::normalize_all(&mut matcher.instance_member_calls);
    matcher
}

pub(super) fn normalize_arguments(matcher: &mut ApiMatcher) {
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

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
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

    pub(crate) fn normalize_all(values: &mut Vec<Self>) {
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

impl CallProvenance {
    pub(crate) fn normalize(&mut self) {
        if let Self::ModuleExport { module } = self {
            *module = module.trim().to_string();
        }
    }
}

impl CallMatcher {
    pub(crate) fn normalize(&mut self) {
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
                    normalize_strings(values)
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
    pub(crate) fn normalize_all(arguments: &mut Vec<Self>) {
        for argument in arguments.iter_mut() {
            argument.matcher.normalize();
        }
        arguments.sort_by_key(|argument| argument.index);
        arguments.dedup();
    }
}
