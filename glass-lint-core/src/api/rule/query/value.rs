use std::collections::BTreeMap;

use super::{MemberChain, QueryBuildError, canonical::CanonicalCollection, limits};

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

fn bounded_strings<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Ok(CanonicalCollection::collect(
        values,
        limits::MAX_STATIC_ALTERNATIVES,
        QueryBuildError::EmptyCollection("static alternatives"),
        "static alternatives",
        |value| {
            let value = value.into();
            if value.trim().is_empty() {
                Err(QueryBuildError::EmptyStaticValue)
            } else {
                Ok(value.trim().to_owned())
            }
        },
    )?
    .into_vec())
}

fn canonical_exact(value: impl Into<String>) -> Result<Vec<String>, QueryBuildError> {
    bounded_strings([value])
}

fn bounded_paths<I, S>(values: I) -> Result<Vec<String>, QueryBuildError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Ok(CanonicalCollection::collect(
        values,
        limits::MAX_STATIC_ALTERNATIVES,
        QueryBuildError::EmptyCollection("rooted expression paths"),
        "rooted expression paths",
        |value| MemberChain::parse(value).map(|chain| chain.as_str().to_owned()),
    )?
    .into_vec())
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

/// Sorted argument constraints with a per-index count index, owned together.
///
/// Invariants (maintained by construction):
/// - The constraint vector stays sorted by `(index, matcher)`.
/// - The count map mirrors the vector: per-index and per-group limits are
///   enforced on push.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub(crate) struct ArgumentConstraints {
    constraints: Vec<ArgumentConstraint>,
    counts: BTreeMap<ArgumentIndex, usize>,
}

impl ArgumentConstraints {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(
        &mut self,
        index: ArgumentIndex,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<(), QueryBuildError> {
        let existing_count = self.counts.get(&index).copied().unwrap_or(0);
        if existing_count >= limits::MAX_PREDICATES_PER_ARGUMENT {
            return Err(QueryBuildError::ExcessivePredicates {
                index: index.get(),
                count: existing_count.saturating_add(1),
            });
        }
        if existing_count == 0 && self.counts.len() >= limits::MAX_ARGUMENT_GROUPS {
            return Err(QueryBuildError::ExcessiveArgumentGroups(
                self.counts.len().saturating_add(1),
            ));
        }
        *self.counts.entry(index).or_insert(0) += 1;
        let constraint = ArgumentConstraint::new(index, matcher);
        let position = self
            .constraints
            .binary_search(&constraint)
            .unwrap_or_else(|position| position);
        self.constraints.insert(position, constraint);
        Ok(())
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, ArgumentConstraint> {
        self.constraints.iter()
    }

    pub(crate) fn as_slice(&self) -> &[ArgumentConstraint] {
        &self.constraints
    }

    pub(crate) fn len(&self) -> usize {
        self.constraints.len()
    }

    #[cfg(test)]
    pub(crate) fn from_constraints(mut constraints: Vec<ArgumentConstraint>) -> Self {
        constraints.sort();
        let mut counts = BTreeMap::new();
        for constraint in &constraints {
            *counts.entry(constraint.arg_index()).or_default() += 1;
        }
        Self {
            constraints,
            counts,
        }
    }
}

#[cfg(test)]
mod tests;
