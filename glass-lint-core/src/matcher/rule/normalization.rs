//! Normalization of matcher argument predicates.
//!
//! Argument fields have their own nested sets and ordering rules. Keeping
//! this pass separate from top-level matcher normalization prevents a matcher
//! from being sorted before its semantic argument shape is canonicalized.

use super::matcher::{
    ApiMatcher, CallProvenance, MemberCallProvenance, MemberReadProvenance, canonical_rooted_chain,
    normalize_class_matchers, normalize_constructor_matchers, normalize_flow_value,
    normalize_flows, normalize_instance_member_calls, normalize_member_chain,
    normalize_member_chains, normalize_returned_member_calls, normalize_returned_member_reads,
    normalize_strings,
};

pub(super) fn normalize(mut matcher: ApiMatcher) -> ApiMatcher {
    normalize_arguments(&mut matcher);
    for call in &mut matcher.calls {
        call.name = call.name.trim().to_string();
        match &mut call.provenance {
            CallProvenance::Any | CallProvenance::Global => {}
            CallProvenance::ModuleExport { module } => *module = module.trim().to_string(),
        }
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
        member_call.chain = normalize_member_chain(&member_call.chain);
        if member_call.provenance == MemberCallProvenance::Rooted {
            member_call.chain = canonical_rooted_chain(&member_call.chain).to_string();
        }
        if let MemberCallProvenance::ModuleNamespace { module } = &mut member_call.provenance {
            *module = module.trim().to_string();
        }
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
    normalize_class_matchers(&mut matcher.classes);
    normalize_constructor_matchers(&mut matcher.constructors);
    normalize_flows(&mut matcher.flows);
    normalize_returned_member_calls(&mut matcher.returned_member_calls);
    normalize_returned_member_reads(&mut matcher.returned_member_reads);
    normalize_instance_member_calls(&mut matcher.instance_member_calls);
    matcher
}

pub(super) fn normalize_arguments(matcher: &mut ApiMatcher) {
    for call in &mut matcher.calls {
        for argument in &mut call.arg_strings {
            normalize_strings(&mut argument.values);
            if let Some(predicate) = &mut argument.predicate {
                normalize_flow_value(predicate);
            }
        }
        call.arg_strings.sort_by(|left, right| {
            left.index
                .cmp(&right.index)
                .then_with(|| left.values.cmp(&right.values))
                .then_with(|| {
                    format!("{:?}", left.predicate).cmp(&format!("{:?}", right.predicate))
                })
        });
        call.arg_strings.dedup();
    }

    for member_call in &mut matcher.member_calls {
        for argument in &mut member_call.arg_strings {
            normalize_strings(&mut argument.values);
            if let Some(predicate) = &mut argument.predicate {
                normalize_flow_value(predicate);
            }
        }
        for argument in &mut member_call.arg_object_keys {
            normalize_strings(&mut argument.keys);
        }
        for argument in &mut member_call.arg_rooted_exprs {
            normalize_member_chains(&mut argument.chains);
        }
        member_call.arg_strings.sort_by(|left, right| {
            left.index
                .cmp(&right.index)
                .then_with(|| left.values.cmp(&right.values))
                .then_with(|| {
                    format!("{:?}", left.predicate).cmp(&format!("{:?}", right.predicate))
                })
        });
        member_call.arg_strings.dedup();
        member_call
            .arg_object_keys
            .sort_by(|left, right| (left.index, &left.keys).cmp(&(right.index, &right.keys)));
        member_call.arg_object_keys.dedup();
        member_call
            .arg_rooted_exprs
            .sort_by(|left, right| (left.index, &left.chains).cmp(&(right.index, &right.chains)));
        member_call.arg_rooted_exprs.dedup();
    }
}
