//! Shared value predicates for fact-driven flow analysis.

use glass_lint_datastructures::{NamePath, NameTable};

use crate::{
    analysis::{
        facts::{ArgumentView, CallArgInfo},
        model::value::{StaticObject, Value, ValueId, ValueTable},
    },
    api::rule::{
        ArgumentMatcher, ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcher,
        ValueMatcherKind,
    },
};

impl ValueMatcher {
    /// Match a value against a known static string without widening unknowns.
    pub(in crate::analysis) fn matches_static(&self, value: &str) -> bool {
        match self.kind() {
            ValueMatcherKind::Any => true,
            ValueMatcherKind::StaticString(predicate) => match predicate.kind() {
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
        match self.kind() {
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
            ArgumentMatcherKind::Value(value) => {
                value.matches_flow_value(argument.static_string(values))
            }
            ArgumentMatcherKind::ObjectKeys(expected) => {
                argument.static_object(values).is_some_and(|object| {
                    expected.iter().all(|expected| {
                        names
                            .lookup(expected.as_str())
                            .is_some_and(|key| object.contains_key(key))
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
                if let Some(object) = argument.static_object(values) {
                    return names
                        .lookup(property.as_str())
                        .and_then(|key| object.get(key))
                        .is_some_and(|value_id| {
                            value.matches_flow_value(values.static_string(value_id))
                        });
                }
                value.matches_flow_value(argument.static_string(values))
            }
        }
    }
}

pub(in crate::analysis) trait ArgumentData {
    fn value(&self) -> ValueId;

    fn static_string<'a>(&'a self, values: &'a ValueTable) -> Option<&'a str> {
        self.overlay_static_string()
            .or_else(|| values.static_string(self.value()))
    }

    fn overlay_static_string(&self) -> Option<&str> {
        None
    }

    fn static_object<'a>(&'a self, values: &'a ValueTable) -> Option<&'a StaticObject> {
        self.prepared_static_object()
            .or_else(|| self.arena_static_object(values))
    }

    fn arena_static_object<'v>(&self, values: &'v ValueTable) -> Option<&'v StaticObject>;

    fn rooted_chain<'a>(&'a self, values: &'a ValueTable) -> Option<&'a NamePath> {
        self.prepared_rooted_chain()
            .or_else(|| self.arena_rooted_chain(values))
    }

    fn arena_rooted_chain<'v>(&self, values: &'v ValueTable) -> Option<&'v NamePath>;

    fn prepared_static_object(&self) -> Option<&StaticObject> {
        None
    }

    fn prepared_rooted_chain(&self) -> Option<&NamePath> {
        None
    }
}

impl ArgumentData for CallArgInfo {
    fn value(&self) -> ValueId {
        self.value
    }

    fn arena_static_object<'v>(&self, values: &'v ValueTable) -> Option<&'v StaticObject> {
        match values.resolve(self.value)? {
            Value::StaticObject(object) => Some(object),
            _ => None,
        }
    }

    fn arena_rooted_chain<'v>(&self, values: &'v ValueTable) -> Option<&'v NamePath> {
        match values.resolve(self.value)? {
            Value::RootedMember { path } => Some(path),
            _ => None,
        }
    }
}

impl ArgumentData for ArgumentView<'_> {
    fn value(&self) -> ValueId {
        self.argument.value()
    }

    fn overlay_static_string(&self) -> Option<&str> {
        self.static_string
    }

    fn arena_static_object<'v>(&self, values: &'v ValueTable) -> Option<&'v StaticObject> {
        self.argument.arena_static_object(values)
    }

    fn arena_rooted_chain<'v>(&self, values: &'v ValueTable) -> Option<&'v NamePath> {
        self.argument.arena_rooted_chain(values)
    }

    fn prepared_static_object(&self) -> Option<&StaticObject> {
        self.object
    }

    fn prepared_rooted_chain(&self) -> Option<&NamePath> {
        self.rooted_chain
    }
}
