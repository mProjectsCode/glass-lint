/// A validated bounded argument position index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArgumentIndex(u8);

impl ArgumentIndex {
    pub(crate) fn new_unchecked(index: u8) -> Self {
        Self(index)
    }

    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueMatcher {
    pub(crate) kind: ValueMatcherKind,
}

impl ValueMatcher {
    pub fn kind(&self) -> &ValueMatcherKind {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ValueMatcherKind {
    Any,
    StaticString(StaticStringPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum StaticStringPredicateKind {
    Any,
    Exact(Vec<String>),
    Prefix(Vec<String>),
    ContainsAny(Vec<String>),
    ContainsAll(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StaticStringPredicate {
    pub(crate) kind: StaticStringPredicateKind,
}

impl StaticStringPredicate {
    pub(crate) fn new(kind: StaticStringPredicateKind) -> Self {
        Self { kind }
    }
}

fn canonicalize_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

impl ValueMatcher {
    #[must_use]
    fn with_static_predicate(mut self, kind: StaticStringPredicateKind) -> Self {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::new(kind));
        self
    }

    #[must_use]
    pub fn any_value() -> Self {
        Self {
            kind: ValueMatcherKind::Any,
        }
    }

    #[must_use]
    pub fn static_string() -> Self {
        Self {
            kind: ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Any,
            )),
        }
    }

    #[must_use]
    pub fn equals(self, value: impl Into<String>) -> Self {
        self.with_static_predicate(StaticStringPredicateKind::Exact(vec![value.into()]))
    }

    #[must_use]
    pub fn equals_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
        canonicalize_strings(&mut values);
        self.with_static_predicate(StaticStringPredicateKind::Exact(values))
    }

    #[must_use]
    pub fn starts_with_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
        canonicalize_strings(&mut values);
        self.with_static_predicate(StaticStringPredicateKind::Prefix(values))
    }

    #[must_use]
    pub fn contains_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
        canonicalize_strings(&mut values);
        self.with_static_predicate(StaticStringPredicateKind::ContainsAny(values))
    }

    #[must_use]
    pub fn contains_all<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
        canonicalize_strings(&mut values);
        self.with_static_predicate(StaticStringPredicateKind::ContainsAll(values))
    }
}

// ── ArgumentMatcher ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ArgumentMatcherKind {
    Value(ValueMatcher),
    ObjectKeys(Vec<String>),
    RootedExpressions(Vec<String>),
    ObjectPropertyValue {
        property: String,
        value: ValueMatcher,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArgumentMatcher {
    pub(crate) kind: ArgumentMatcherKind,
}

impl ArgumentMatcher {
    pub(crate) fn kind(&self) -> &ArgumentMatcherKind {
        &self.kind
    }

    pub fn object_keys<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            kind: ArgumentMatcherKind::ObjectKeys(keys.into_iter().map(Into::into).collect()),
        }
    }

    pub fn rooted_expressions<I, S>(chains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            kind: ArgumentMatcherKind::RootedExpressions(
                chains.into_iter().map(Into::into).collect(),
            ),
        }
    }

    pub fn object_property_value(property: impl Into<String>, value: ValueMatcher) -> Self {
        Self {
            kind: ArgumentMatcherKind::ObjectPropertyValue {
                property: property.into(),
                value,
            },
        }
    }
}

impl From<ValueMatcher> for ArgumentMatcher {
    fn from(value: ValueMatcher) -> Self {
        Self {
            kind: ArgumentMatcherKind::Value(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArgumentConstraint {
    index: usize,
    matcher: ArgumentMatcher,
}

impl ArgumentConstraint {
    pub fn new(index: ArgumentIndex, matcher: impl Into<ArgumentMatcher>) -> Self {
        Self {
            index: index.get(),
            matcher: matcher.into(),
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn arg_index(&self) -> ArgumentIndex {
        ArgumentIndex::new_unchecked(self.index as u8)
    }

    pub fn matcher(&self) -> &ArgumentMatcher {
        &self.matcher
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_matcher_any_value_kind_is_any() {
        let m = ValueMatcher::any_value();
        assert_eq!(m.kind(), &ValueMatcherKind::Any);
    }

    #[test]
    fn value_matcher_static_string_default_is_any() {
        let m = ValueMatcher::static_string();
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Any
            ))
        );
    }

    #[test]
    fn value_matcher_equals_creates_exact_predicate() {
        let m = ValueMatcher::static_string().equals("hello");
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Exact(vec!["hello".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_equals_any_creates_multi_exact() {
        let m = ValueMatcher::static_string().equals_any(["a", "b"]);
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Exact(vec!["a".into(), "b".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_starts_with_any_creates_prefix_predicate() {
        let m = ValueMatcher::static_string().starts_with_any(["https://"]);
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Prefix(vec!["https://".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_contains_any_creates_contains_any() {
        let m = ValueMatcher::static_string().contains_any(["token", "secret"]);
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::ContainsAny(vec!["secret".into(), "token".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_contains_all_creates_contains_all() {
        let m = ValueMatcher::static_string().contains_all(["required", "field"]);
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::ContainsAll(vec!["field".into(), "required".into()])
            ))
        );
    }

    #[test]
    fn static_string_predicate_new_round_trips_kind() {
        let p = StaticStringPredicate::new(StaticStringPredicateKind::Any);
        assert!(matches!(p.kind, StaticStringPredicateKind::Any));
    }

    #[test]
    fn argument_matcher_object_keys_holds_keys() {
        let m = ArgumentMatcher::object_keys(["x", "y"]);
        assert!(matches!(m.kind(), ArgumentMatcherKind::ObjectKeys(keys) if keys == &["x", "y"]));
    }

    #[test]
    fn argument_matcher_rooted_expressions_holds_chains() {
        let m = ArgumentMatcher::rooted_expressions(["document.body"]);
        assert!(
            matches!(m.kind(), ArgumentMatcherKind::RootedExpressions(chains) if chains == &["document.body"])
        );
    }

    #[test]
    fn argument_matcher_object_property_value_holds_property_and_matcher() {
        let value = ValueMatcher::static_string().equals("file");
        let m = ArgumentMatcher::object_property_value("type", value);
        assert!(
            matches!(m.kind(), ArgumentMatcherKind::ObjectPropertyValue { property, .. } if property == "type")
        );
    }

    #[test]
    fn argument_matcher_from_value_matcher_converts() {
        let vm = ValueMatcher::any_value();
        let m: ArgumentMatcher = vm.into();
        assert!(matches!(m.kind(), ArgumentMatcherKind::Value(_)));
    }

    #[test]
    fn argument_constraint_new_holds_index_and_matcher() {
        let m = ArgumentMatcher::object_keys(["k"]);
        let c = ArgumentConstraint::new(ArgumentIndex::new_unchecked(2), m);
        assert_eq!(c.index(), 2);
        assert!(matches!(
            c.matcher().kind(),
            ArgumentMatcherKind::ObjectKeys(_)
        ));
    }
}
