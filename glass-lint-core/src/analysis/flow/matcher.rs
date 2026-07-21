//! Shared value predicates for fact-driven flow analysis.

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo},
        name::NameTable,
    },
    api::rule::{ArgumentMatcher, StaticStringPredicate, ValueMatcher, ValueMatcherKind},
};

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
        // A value predicate cannot prove a dynamic string, so absence of a
        // static value is a non-match even when the predicate is selective.
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
    pub(in crate::analysis) fn matches<T: ArgumentData>(
        &self,
        argument: &T,
        names: &NameTable,
    ) -> bool {
        match self {
            Self::Value(value) => value.matches_flow_value(argument.static_string()),
            Self::ObjectKeys(expected) => argument.object_keys().is_some_and(|keys| {
                expected.iter().all(|expected| {
                    keys.iter()
                        .any(|key| names.resolve(*key) == Some(expected.as_str()))
                })
            }),
            Self::RootedExpressions(expected) => argument.rooted_chain().is_some_and(|chain| {
                let Some(chain) = chain.to_symbol_path(names) else {
                    return false;
                };
                expected.iter().any(|candidate| chain.eq_chain(candidate))
            }),
            Self::ObjectPropertyValue { property, value } => {
                argument.property_strings().iter().any(|(found, string)| {
                    names.resolve(*found) == Some(property.as_str())
                        && value.matches_flow_value(Some(string))
                })
            }
        }
    }
}

pub(in crate::analysis) trait ArgumentData {
    fn static_string(&self) -> Option<&str>;
    fn object_keys(&self) -> Option<&Vec<crate::analysis::name::NameId>>;
    fn rooted_chain(&self) -> Option<&crate::analysis::value::NamePath>;
    fn property_strings(&self) -> &Vec<(crate::analysis::name::NameId, String)>;
}

impl ArgumentData for CallArgInfo {
    fn static_string(&self) -> Option<&str> {
        self.static_string.as_deref()
    }

    fn object_keys(&self) -> Option<&Vec<crate::analysis::name::NameId>> {
        self.object_keys.as_ref()
    }

    fn rooted_chain(&self) -> Option<&crate::analysis::value::NamePath> {
        self.rooted_chain.as_ref()
    }

    fn property_strings(&self) -> &Vec<(crate::analysis::name::NameId, String)> {
        &self.property_strings
    }
}

impl ArgumentData for ArgumentView<'_> {
    fn static_string(&self) -> Option<&str> {
        self.static_string.or_else(|| self.argument.static_string())
    }

    fn object_keys(&self) -> Option<&Vec<crate::analysis::name::NameId>> {
        self.argument.object_keys()
    }

    fn rooted_chain(&self) -> Option<&crate::analysis::value::NamePath> {
        self.argument.rooted_chain()
    }

    fn property_strings(&self) -> &Vec<(crate::analysis::name::NameId, String)> {
        self.argument.property_strings()
    }
}
