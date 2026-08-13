use std::collections::BTreeMap;

use super::{QueryBuildError, checked_chain, limits};

/// A validated authored argument position in a call query.
///
/// This semantic position is lowered into private physical slots; it is not a
/// slot identity and must not be compared with compiler artifact IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArgumentIndex(u8);

impl ArgumentIndex {
    pub(crate) fn new_unchecked(index: u8) -> Self {
        Self(index)
    }

    pub(crate) fn try_from_usize(index: usize) -> Result<Self, QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let index =
            u8::try_from(index).map_err(|_| QueryBuildError::InvalidArgumentIndex(index))?;
        Ok(Self::new_unchecked(index))
    }

    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueMatcher {
    kind: ValueMatcherKind,
}

impl ValueMatcher {
    pub(crate) fn kind(&self) -> &ValueMatcherKind {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ValueMatcherKind {
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
pub(crate) struct StaticStringPredicate {
    kind: StaticStringPredicateKind,
}

impl StaticStringPredicate {
    pub(crate) fn new(kind: StaticStringPredicateKind) -> Self {
        Self { kind }
    }

    pub(crate) fn kind(&self) -> &StaticStringPredicateKind {
        &self.kind
    }
}

fn canonicalize_strings(values: &mut Vec<String>) {
    for value in values.iter_mut() {
        *value = value.trim().to_owned();
    }
    values.sort();
    values.dedup();
}

fn bounded_canonical_values<I, S, F>(
    values: I,
    empty_label: &'static str,
    mut parse: F,
) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    F: FnMut(S) -> Result<String, QueryBuildError>,
{
    let mut parsed: Vec<String> = Vec::new();
    for value in values {
        if parsed.len() >= limits::MAX_STATIC_ALTERNATIVES {
            return Err(QueryBuildError::CollectionTooLarge(
                empty_label,
                parsed.len() + 1,
            ));
        }
        parsed.push(parse(value)?);
    }
    canonicalize_strings(&mut parsed);
    if parsed.is_empty() {
        return Err(QueryBuildError::EmptyCollection(empty_label));
    }
    if parsed.len() > limits::MAX_STATIC_ALTERNATIVES {
        return Err(QueryBuildError::CollectionTooLarge(
            empty_label,
            parsed.len(),
        ));
    }
    Ok(parsed)
}

fn bounded_strings<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    bounded_canonical_values(values, "static alternatives", |value| {
        let value = value.into();
        if value.trim().is_empty() {
            Err(QueryBuildError::EmptyStaticValue)
        } else {
            Ok(value)
        }
    })
}

fn canonical_exact(value: impl Into<String>) -> Result<Vec<String>, QueryBuildError> {
    bounded_strings([value])
}

fn bounded_paths<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    bounded_canonical_values(values, "rooted expression paths", |value| {
        checked_chain(value).map(|chain| chain.as_str().to_owned())
    })
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

    pub fn try_equals(self, value: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(self.with_static_predicate(StaticStringPredicateKind::Exact(canonical_exact(value)?)))
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
    kind: ArgumentMatcherKind,
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
    index: ArgumentIndex,
    matcher: ArgumentMatcher,
}

impl ArgumentConstraint {
    pub fn new(index: ArgumentIndex, matcher: impl Into<ArgumentMatcher>) -> Self {
        Self {
            index,
            matcher: matcher.into(),
        }
    }

    pub fn arg_index(&self) -> ArgumentIndex {
        self.index
    }

    pub fn predicate(&self) -> &ArgumentMatcher {
        &self.matcher
    }
}

pub(crate) fn push_argument_constraint(
    constraints: &mut Vec<ArgumentConstraint>,
    counts: &mut BTreeMap<ArgumentIndex, usize>,
    index: ArgumentIndex,
    matcher: impl Into<ArgumentMatcher>,
) -> Result<(), QueryBuildError> {
    let existing_count = counts.get(&index).copied().unwrap_or(0);
    if existing_count >= limits::MAX_PREDICATES_PER_ARGUMENT {
        return Err(QueryBuildError::ExcessivePredicates {
            index: index.get(),
            count: existing_count.saturating_add(1),
        });
    }
    if existing_count == 0 && counts.len() >= limits::MAX_ARGUMENT_GROUPS {
        return Err(QueryBuildError::ExcessiveArgumentGroups(
            counts.len().saturating_add(1),
        ));
    }
    *counts.entry(index).or_insert(0) += 1;
    let constraint = ArgumentConstraint::new(index, matcher);
    let position = constraints
        .binary_search(&constraint)
        .unwrap_or_else(|position| position);
    constraints.insert(position, constraint);
    Ok(())
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
        let m = ValueMatcher::static_string().try_equals("hello").unwrap();
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
    fn value_matcher_equals_uses_canonical_static_values() {
        let exact = ValueMatcher::static_string().try_equals(" x ").unwrap();
        let alternatives = ValueMatcher::static_string().equals_any(["x"]).unwrap();
        assert_eq!(exact, alternatives);
    }

    #[test]
    fn value_matcher_try_equals_rejects_empty_values() {
        assert_eq!(
            ValueMatcher::static_string().try_equals(" "),
            Err(QueryBuildError::EmptyStaticValue)
        );
        assert_eq!(
            ValueMatcher::static_string().try_equals(""),
            Err(QueryBuildError::EmptyStaticValue)
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
        let value = ValueMatcher::static_string().try_equals("file").unwrap();
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
        assert_eq!(c.arg_index().get(), 2);
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
