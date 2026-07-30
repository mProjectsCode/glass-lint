use std::collections::BTreeMap;

use super::{QueryBuildError, limits};

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
    for value in values.iter_mut() {
        *value = value.trim().to_owned();
    }
    values.sort();
    values.dedup();
}

fn bounded_strings<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(QueryBuildError::EmptyStaticValue);
    }
    canonicalize_strings(&mut values);
    if values.is_empty() {
        return Err(QueryBuildError::EmptyCollection("static alternatives"));
    }
    if values.len() > limits::MAX_STATIC_ALTERNATIVES {
        return Err(QueryBuildError::CollectionTooLarge(
            "static alternatives",
            values.len(),
        ));
    }
    Ok(values)
}

fn bounded_paths<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let values: Vec<String> = values.into_iter().map(Into::into).collect();
    if values.iter().any(|value| {
        let trimmed = value.trim();
        trimmed.is_empty()
            || trimmed.starts_with('.')
            || trimmed.ends_with('.')
            || trimmed.contains("..")
    }) {
        return Err(QueryBuildError::MalformedChain(
            "invalid rooted expression path".into(),
        ));
    }
    let mut values = values;
    canonicalize_strings(&mut values);
    if values.is_empty() {
        return Err(QueryBuildError::EmptyCollection("rooted expression paths"));
    }
    if values.len() > limits::MAX_STATIC_ALTERNATIVES {
        return Err(QueryBuildError::CollectionTooLarge(
            "rooted expression paths",
            values.len(),
        ));
    }
    Ok(values)
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

    pub fn equals_any<I, S>(self, values: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(self.with_static_predicate(StaticStringPredicateKind::Exact(bounded_strings(values)?)))
    }

    pub fn starts_with_any<I, S>(self, values: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(self.with_static_predicate(StaticStringPredicateKind::Prefix(bounded_strings(values)?)))
    }

    pub fn contains_any<I, S>(self, values: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(
            self.with_static_predicate(StaticStringPredicateKind::ContainsAny(bounded_strings(
                values,
            )?)),
        )
    }

    pub fn contains_all<I, S>(self, values: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(
            self.with_static_predicate(StaticStringPredicateKind::ContainsAll(bounded_strings(
                values,
            )?)),
        )
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

    pub fn object_keys<I, S>(keys: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(Self {
            kind: ArgumentMatcherKind::ObjectKeys(bounded_strings(keys)?),
        })
    }

    pub fn rooted_expressions<I, S>(chains: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Ok(Self {
            kind: ArgumentMatcherKind::RootedExpressions(bounded_paths(chains)?),
        })
    }

    pub fn object_property_value(
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Result<Self, QueryBuildError> {
        let property = property.into();
        if property.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            kind: ArgumentMatcherKind::ObjectPropertyValue {
                property: property.trim().to_owned(),
                value,
            },
        })
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

    pub fn predicate(&self) -> &ArgumentMatcher {
        &self.matcher
    }
}

/// Incrementally validates and canonicalizes argument constraint groups.
#[derive(Debug, Default)]
pub(crate) struct ArgumentConstraintsBuilder {
    constraints: Vec<ArgumentConstraint>,
    counts: BTreeMap<usize, usize>,
}

impl ArgumentConstraintsBuilder {
    pub(crate) fn from_constraints(
        constraints: &[ArgumentConstraint],
    ) -> Result<Self, QueryBuildError> {
        let mut builder = Self::default();
        for constraint in constraints {
            builder.push(constraint.arg_index().get(), constraint.predicate().clone())?;
        }
        Ok(builder)
    }

    pub(crate) fn push(
        &mut self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<(), QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let existing_count = self.counts.get(&index).copied().unwrap_or(0);
        if existing_count >= limits::MAX_PREDICATES_PER_ARGUMENT {
            return Err(QueryBuildError::ExcessivePredicates {
                index,
                count: existing_count.saturating_add(1),
            });
        }
        if existing_count == 0 && self.counts.len() >= limits::MAX_ARGUMENT_GROUPS {
            return Err(QueryBuildError::ExcessiveArgumentGroups(
                self.counts.len().saturating_add(1),
            ));
        }
        *self.counts.entry(index).or_insert(0) += 1;
        self.constraints.push(ArgumentConstraint::new(
            ArgumentIndex::new_unchecked(index as u8),
            matcher,
        ));
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Vec<ArgumentConstraint> {
        self.constraints.sort_unstable();
        self.constraints
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
        let m = ValueMatcher::static_string()
            .equals_any(["a", "b"])
            .unwrap();
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Exact(vec!["a".into(), "b".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_starts_with_any_creates_prefix_predicate() {
        let m = ValueMatcher::static_string()
            .starts_with_any(["https://"])
            .unwrap();
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::Prefix(vec!["https://".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_contains_any_creates_contains_any() {
        let m = ValueMatcher::static_string()
            .contains_any(["token", "secret"])
            .unwrap();
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::ContainsAny(vec!["secret".into(), "token".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_contains_all_creates_contains_all() {
        let m = ValueMatcher::static_string()
            .contains_all(["required", "field"])
            .unwrap();
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
        let m = ArgumentMatcher::object_keys(["x", "y"]).unwrap();
        assert!(matches!(m.kind(), ArgumentMatcherKind::ObjectKeys(keys) if keys == &["x", "y"]));
    }

    #[test]
    fn argument_matcher_rooted_expressions_holds_chains() {
        let m = ArgumentMatcher::rooted_expressions(["document.body"]).unwrap();
        assert!(
            matches!(m.kind(), ArgumentMatcherKind::RootedExpressions(chains) if chains == &["document.body"])
        );
    }

    #[test]
    fn argument_matcher_object_property_value_holds_property_and_matcher() {
        let value = ValueMatcher::static_string().equals("file");
        let m = ArgumentMatcher::object_property_value("type", value).unwrap();
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
        let m = ArgumentMatcher::object_keys(["k"]).unwrap();
        let c = ArgumentConstraint::new(ArgumentIndex::new_unchecked(2), m);
        assert_eq!(c.index(), 2);
        assert!(matches!(
            c.predicate().kind(),
            ArgumentMatcherKind::ObjectKeys(_)
        ));
    }

    #[test]
    fn object_key_collections_are_non_empty_canonical_and_bounded() {
        let matcher = ArgumentMatcher::object_keys(["method", "url", "url"]).unwrap();
        assert!(matches!(
            matcher.kind(),
            ArgumentMatcherKind::ObjectKeys(keys)
                if keys == &["method".to_string(), "url".to_string()]
        ));
        assert!(matches!(
            ArgumentMatcher::object_keys::<[&str; 0], &str>([]),
            Err(QueryBuildError::EmptyCollection(_))
        ));
        let keys: Vec<String> = (0..=limits::MAX_STATIC_ALTERNATIVES)
            .map(|index| format!("key{index}"))
            .collect();
        assert!(matches!(
            ArgumentMatcher::object_keys(keys),
            Err(QueryBuildError::CollectionTooLarge(_, _))
        ));
    }

    #[test]
    fn rooted_expression_collections_validate_paths_and_limits() {
        let matcher =
            ArgumentMatcher::rooted_expressions(["document.body", "document.body"]).unwrap();
        assert!(matches!(
            matcher.kind(),
            ArgumentMatcherKind::RootedExpressions(paths)
                if paths == &["document.body".to_string()]
        ));
        assert!(matches!(
            ArgumentMatcher::rooted_expressions(["document..body"]),
            Err(QueryBuildError::MalformedChain(_))
        ));
        let paths: Vec<String> = (0..=limits::MAX_STATIC_ALTERNATIVES)
            .map(|index| format!("document.node{index}"))
            .collect();
        assert!(matches!(
            ArgumentMatcher::rooted_expressions(paths),
            Err(QueryBuildError::CollectionTooLarge(_, _))
        ));
    }
}
