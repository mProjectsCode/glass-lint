//! Shared value predicates for fact-driven flow analysis.

use crate::api::rule::FlowValueMatcher;

pub(in crate::analysis) fn matches_static_value(matcher: &FlowValueMatcher, value: &str) -> bool {
    match matcher {
        FlowValueMatcher::Any => true,
        FlowValueMatcher::StaticExact(values) => values.iter().any(|expected| expected == value),
        FlowValueMatcher::StaticPrefix(prefixes) => {
            prefixes.iter().any(|prefix| value.starts_with(prefix))
        }
        FlowValueMatcher::StaticContainsAny(markers) => {
            markers.iter().any(|marker| value.contains(marker))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            markers.iter().all(|marker| value.contains(marker))
        }
    }
}

pub(in crate::analysis) fn flow_value_matches(
    matcher: &FlowValueMatcher,
    static_value: Option<&str>,
    allow_dynamic_for_any: bool,
) -> bool {
    match matcher {
        FlowValueMatcher::Any => allow_dynamic_for_any || static_value.is_some(),
        FlowValueMatcher::StaticExact(values) => {
            static_value.is_some_and(|value| values.iter().any(|expected| expected == value))
        }
        FlowValueMatcher::StaticPrefix(prefixes) => static_value
            .is_some_and(|value| prefixes.iter().any(|prefix| value.starts_with(prefix))),
        FlowValueMatcher::StaticContainsAny(markers) => {
            static_value.is_some_and(|value| markers.iter().any(|marker| value.contains(marker)))
        }
        FlowValueMatcher::StaticContainsAll(markers) => {
            static_value.is_some_and(|value| markers.iter().all(|marker| value.contains(marker)))
        }
    }
}
