//! Shared value predicates for fact-driven flow analysis.

use glass_lint_datastructures::NameTable;

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo},
        value::{Value, ValueTable},
    },
    api::rule::{
        ArgumentMatcher, ValueMatcher, ValueMatcherKind,
        matcher::{ArgumentMatcherKind, StaticStringPredicateKind},
    },
};

impl ValueMatcher {
    /// Match a value against a known static string without widening unknowns.
    pub(in crate::analysis) fn matches_static(&self, value: &str) -> bool {
        match &self.kind {
            ValueMatcherKind::Any => true,
            ValueMatcherKind::StaticString(predicate) => match &predicate.kind {
                StaticStringPredicateKind::Any => true,
                StaticStringPredicateKind::Exact(values) => {
                    values.iter().any(|expected| expected == value)
                }
                StaticStringPredicateKind::Prefix(prefixes) => {
                    prefixes.iter().any(|prefix| value.starts_with(prefix))
                }
                StaticStringPredicateKind::ContainsAny(markers) => {
                    markers.iter().any(|marker| value.contains(marker))
                }
                StaticStringPredicateKind::ContainsAll(markers) => {
                    markers.iter().all(|marker| value.contains(marker))
                }
            },
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
    ///
    /// Static strings, object keys, rooted chains, and property values are
    /// all derived from the frozen [`ValueTable`] through the argument's
    /// [`ValueId`].
    pub(in crate::analysis) fn matches<T: ArgumentData>(
        &self,
        argument: &T,
        names: &NameTable,
        values: &ValueTable,
    ) -> bool {
        match self.kind() {
            ArgumentMatcherKind::Value(value) => value.matches_flow_value(
                argument
                    .overlay_static_string()
                    .or_else(|| values.static_string(argument.value())),
            ),
            ArgumentMatcherKind::ObjectKeys(expected) => {
                argument.object_entries(values).is_some_and(|entries| {
                    expected.iter().all(|expected| {
                        entries
                            .iter()
                            .any(|(key, _)| names.resolve(*key) == Some(expected.as_str()))
                    })
                })
            }
            ArgumentMatcherKind::RootedExpressions(expected) => {
                argument.rooted_chain(values).is_some_and(|chain| {
                    let Some(chain) = names.resolve_path(chain) else {
                        return false;
                    };
                    expected.iter().any(|candidate| chain.eq_chain(candidate))
                })
            }
            ArgumentMatcherKind::ObjectPropertyValue { property, value } => {
                let val = argument.value();
                let entry = values.resolve(val);
                entry.is_some_and(|e| match e {
                    Value::StaticObject(entries) => entries.iter().any(|(name_id, value_id)| {
                        names.resolve(*name_id) == Some(property.as_str())
                            && value.matches_flow_value(values.static_string(*value_id))
                    }),
                    _ => value.matches_flow_value(
                        argument
                            .overlay_static_string()
                            .or_else(|| values.static_string(argument.value())),
                    ),
                })
            }
        }
    }
}

pub(in crate::analysis) trait ArgumentData {
    fn value(&self) -> crate::analysis::value::ValueId;
    fn overlay_static_string(&self) -> Option<&str> {
        None
    }
    fn object_entries<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<
        &'v [(
            glass_lint_datastructures::NameId,
            crate::analysis::value::ValueId,
        )],
    >;
    fn rooted_chain<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<&'v glass_lint_datastructures::NamePath>;
}

impl ArgumentData for CallArgInfo {
    fn value(&self) -> crate::analysis::value::ValueId {
        self.value
    }

    fn object_entries<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<
        &'v [(
            glass_lint_datastructures::NameId,
            crate::analysis::value::ValueId,
        )],
    > {
        match values.resolve(self.value)? {
            Value::StaticObject(entries) => Some(entries.as_slice()),
            _ => None,
        }
    }

    fn rooted_chain<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<&'v glass_lint_datastructures::NamePath> {
        match values.resolve(self.value)? {
            Value::RootedMember { path } => Some(path),
            _ => None,
        }
    }
}

impl ArgumentData for ArgumentView<'_> {
    fn value(&self) -> crate::analysis::value::ValueId {
        self.argument.value()
    }

    fn overlay_static_string(&self) -> Option<&str> {
        self.static_string
    }

    fn object_entries<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<
        &'v [(
            glass_lint_datastructures::NameId,
            crate::analysis::value::ValueId,
        )],
    > {
        self.argument.object_entries(values)
    }

    fn rooted_chain<'v>(
        &self,
        values: &'v ValueTable,
    ) -> Option<&'v glass_lint_datastructures::NamePath> {
        self.argument.rooted_chain(values)
    }
}
