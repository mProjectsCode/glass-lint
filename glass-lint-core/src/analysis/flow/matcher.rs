//! Shared value predicates for fact-driven flow analysis.

use crate::analysis::facts::CallArgInfo;
use crate::api::rule::StaticStringPredicate;
use crate::api::rule::{ArgumentMatcher, MemberCallProvenance, ValueMatcher, ValueMatcherKind};

pub(in crate::analysis) fn matches_static_value(matcher: &ValueMatcher, value: &str) -> bool {
    match &matcher.kind {
        ValueMatcherKind::Any => true,
        ValueMatcherKind::StaticString(StaticStringPredicate::Any) => true,
        ValueMatcherKind::StaticString(StaticStringPredicate::Exact(values)) => {
            values.iter().any(|expected| expected == value)
        }
        ValueMatcherKind::StaticString(StaticStringPredicate::Prefix(prefixes)) => {
            prefixes.iter().any(|prefix| value.starts_with(prefix))
        }
        ValueMatcherKind::StaticString(StaticStringPredicate::ContainsAny(markers)) => {
            markers.iter().any(|marker| value.contains(marker))
        }
        ValueMatcherKind::StaticString(StaticStringPredicate::ContainsAll(markers)) => {
            markers.iter().all(|marker| value.contains(marker))
        }
    }
}

pub(in crate::analysis) fn flow_value_matches(
    matcher: &ValueMatcher,
    static_value: Option<&str>,
) -> bool {
    match &matcher.kind {
        ValueMatcherKind::Any => true,
        ValueMatcherKind::StaticString(_) => {
            static_value.is_some_and(|value| matches_static_value(matcher, value))
        }
    }
}

pub(in crate::analysis) fn argument_matches(
    matcher: &ArgumentMatcher,
    argument: &CallArgInfo,
) -> bool {
    match matcher {
        ArgumentMatcher::Value(value) => {
            flow_value_matches(value, argument.static_string.as_deref())
        }
        ArgumentMatcher::ObjectKeys(expected) => {
            argument.object_keys.as_ref().is_some_and(|keys| {
                expected
                    .iter()
                    .all(|expected| keys.iter().any(|key| key == expected))
            })
        }
        ArgumentMatcher::RootedExpressions(expected) => {
            argument.rooted_chain.as_ref().is_some_and(|chain| {
                let chain = crate::api::rule::canonical_rooted_chain(chain);
                expected.iter().any(|candidate| candidate == chain)
            })
        }
    }
}

pub(in crate::analysis) fn member_call_matches_provenance(
    matcher: &MemberCallProvenance,
    rooted: bool,
) -> bool {
    match matcher {
        MemberCallProvenance::Any => true,
        MemberCallProvenance::Rooted => rooted,
        MemberCallProvenance::ModuleNamespace { .. } => false,
    }
}
