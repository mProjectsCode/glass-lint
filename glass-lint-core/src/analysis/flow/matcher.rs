//! Shared value predicates for fact-driven flow analysis.

use crate::analysis::facts::CallArgInfo;
use crate::api::rule::StaticStringPredicate;
use crate::api::rule::{ArgumentMatcher, MemberCallProvenance, ValueMatcher, ValueMatcherKind};

impl ValueMatcher {
    /// Match a value against a known static string without widening unknowns.
    pub(in crate::analysis) fn matches_static(&self, value: &str) -> bool {
        match &self.kind {
            ValueMatcherKind::Any | ValueMatcherKind::StaticString(StaticStringPredicate::Any) => {
                true
            }
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

    /// Match a flow value whose static string may be unavailable.
    pub(in crate::analysis) fn matches_flow_value(&self, static_value: Option<&str>) -> bool {
        match &self.kind {
            ValueMatcherKind::Any => true,
            ValueMatcherKind::StaticString(_) => {
                static_value.is_some_and(|value| self.matches_static(value))
            }
        }
    }
}

impl ArgumentMatcher {
    /// Match a pre-computed call argument without consulting the AST.
    pub(in crate::analysis) fn matches(&self, argument: &CallArgInfo) -> bool {
        match self {
            Self::Value(value) => value.matches_flow_value(argument.static_string.as_deref()),
            Self::ObjectKeys(expected) => argument.object_keys.as_ref().is_some_and(|keys| {
                expected
                    .iter()
                    .all(|expected| keys.iter().any(|key| key == expected))
            }),
            Self::RootedExpressions(expected) => {
                argument.rooted_chain.as_ref().is_some_and(|chain| {
                    let chain = crate::api::rule::canonical_rooted_chain(chain);
                    expected.iter().any(|candidate| candidate == chain)
                })
            }
        }
    }
}

impl MemberCallProvenance {
    pub(in crate::analysis) fn matches_rooted(&self, rooted: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Rooted => rooted,
            Self::ModuleNamespace { .. } => false,
        }
    }
}
