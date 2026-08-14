use std::collections::BTreeMap;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::{
    FirstError,
    query::{
        EventQuery, MemberChain, QueryBuildError, checked_chain, limits,
        value::{ArgumentConstraint, ArgumentMatcher, ValueMatcher},
    },
};

/// The identity kind of a lifecycle call endpoint. This is parsed when the
/// query is authored and remains typed through normalization and execution;
/// later phases never infer identity from the endpoint's display spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleCallTarget {
    Global(SmolStr),
    RootedMember(SymbolPath),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleCallEndpoint {
    chain: MemberChain,
    target: LifecycleCallTarget,
}

impl LifecycleCallEndpoint {
    fn new(chain: MemberChain, target: LifecycleCallTarget) -> Self {
        Self { chain, target }
    }

    pub(crate) fn target(&self) -> &LifecycleCallTarget {
        &self.target
    }

    pub(crate) fn chain(&self) -> &str {
        self.chain.as_str()
    }
}

macro_rules! define_lifecycle_adapter {
    ($trait_name:ident, $method:ident, $value:ty) => {
        #[doc = "Sealed fallible lifecycle input adapter."]
        pub trait $trait_name: private::Sealed {
            fn $method(self) -> Result<$value, QueryBuildError>;
        }

        impl $trait_name for $value {
            fn $method(self) -> Result<$value, QueryBuildError> {
                Ok(self)
            }
        }

        impl private::Sealed for $value {}

        impl $trait_name for Result<$value, QueryBuildError> {
            fn $method(self) -> Result<$value, QueryBuildError> {
                self
            }
        }

        impl private::Sealed for Result<$value, QueryBuildError> {}
    };
}

// ── LifecycleEvent ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleEventKind {
    PropertyWrite {
        property: SmolStr,
        value: ValueMatcher,
    },
    MemberCall {
        member: MemberChain,
        arguments: Vec<ArgumentConstraint>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleEvent {
    pub(crate) kind: LifecycleEventKind,
}

impl LifecycleEvent {
    pub(crate) fn kind(&self) -> &LifecycleEventKind {
        &self.kind
    }

    pub fn property_write(
        property: impl Into<SmolStr>,
        value: ValueMatcher,
    ) -> Result<Self, QueryBuildError> {
        let property = property.into();
        if property.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            kind: LifecycleEventKind::PropertyWrite { property, value },
        })
    }

    pub fn member_call(
        member: impl Into<String>,
    ) -> Result<LifecycleEventBuilder, QueryBuildError> {
        let member = member.into();
        if member.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        let member = checked_chain(member)?;
        Ok(LifecycleEventBuilder {
            event: LifecycleEventKind::MemberCall {
                member,
                arguments: Vec::new(),
            },
            argument_counts: BTreeMap::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleEventBuilder {
    event: LifecycleEventKind,
    argument_counts: BTreeMap<super::value::ArgumentIndex, usize>,
}

impl LifecycleEventBuilder {
    pub fn arg(
        mut self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        let index = super::value::ArgumentIndex::try_from_usize(index)?;
        if let LifecycleEventKind::MemberCall { arguments, .. } = &mut self.event {
            super::value::push_argument_constraint(
                arguments,
                &mut self.argument_counts,
                index,
                matcher,
            )?;
        }
        Ok(self)
    }

    pub fn build(self) -> LifecycleEvent {
        LifecycleEvent { kind: self.event }
    }
}

define_lifecycle_adapter!(IntoLifecycleEvent, into_lifecycle_event, LifecycleEvent);

impl IntoLifecycleEvent for LifecycleEventBuilder {
    fn into_lifecycle_event(self) -> Result<LifecycleEvent, QueryBuildError> {
        Ok(self.build())
    }
}

// ── LifecycleCondition ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleConditionKind {
    AnyOf(LifecycleEvents),
    AllOf(LifecycleEvents),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleCondition {
    kind: LifecycleConditionKind,
}

/// Non-empty, bounded, deterministic lifecycle event collections.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct CanonicalLifecycleItems<T>(Box<[T]>);

impl<T: Ord> CanonicalLifecycleItems<T> {
    fn new(
        mut items: Vec<T>,
        empty: QueryBuildError,
        label: &'static str,
        limit: usize,
    ) -> Result<Self, QueryBuildError> {
        if items.is_empty() {
            return Err(empty);
        }
        items.sort();
        items.dedup();
        if items.len() > limit {
            return Err(QueryBuildError::CollectionTooLarge(label, items.len()));
        }
        Ok(Self(items.into_boxed_slice()))
    }

    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleEvents(CanonicalLifecycleItems<LifecycleEvent>);

impl LifecycleEvents {
    fn new(events: Vec<LifecycleEvent>) -> Result<Self, QueryBuildError> {
        Ok(Self(CanonicalLifecycleItems::new(
            events,
            QueryBuildError::EmptyLifecycleCondition,
            "lifecycle condition events",
            limits::MAX_LIFECYCLE_EVENTS,
        )?))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LifecycleEvent> {
        self.0.iter()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

fn bounded_lifecycle_items<I, T, F>(
    items: I,
    label: &'static str,
    mut convert: F,
) -> Result<Vec<T>, QueryBuildError>
where
    I: IntoIterator,
    F: FnMut(I::Item) -> Result<T, QueryBuildError>,
{
    let mut converted = Vec::new();
    for item in items {
        if converted.len() >= limits::MAX_LIFECYCLE_EVENTS {
            return Err(QueryBuildError::CollectionTooLarge(
                label,
                converted.len() + 1,
            ));
        }
        converted.push(convert(item)?);
    }
    Ok(converted)
}

impl LifecycleCondition {
    pub(crate) fn kind(&self) -> &LifecycleConditionKind {
        &self.kind
    }

    pub fn any_of<I>(events: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleEvent,
    {
        let events = bounded_lifecycle_items(
            events,
            "lifecycle condition events",
            IntoLifecycleEvent::into_lifecycle_event,
        )?;
        Ok(Self {
            kind: LifecycleConditionKind::AnyOf(LifecycleEvents::new(events)?),
        })
    }

    /// Require every event on the same tracked lifecycle object.
    ///
    /// This is a bounded multi-event correlation. It preserves path-local
    /// identity: an event from another object, an incompatible branch, an
    /// unknown value, or an exhausted alternative cannot complete the
    /// conjunction.
    pub fn all_of<I>(events: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator,
        I::Item: IntoLifecycleEvent,
    {
        let events = bounded_lifecycle_items(
            events,
            "lifecycle condition events",
            IntoLifecycleEvent::into_lifecycle_event,
        )?;
        Ok(Self {
            kind: LifecycleConditionKind::AllOf(LifecycleEvents::new(events)?),
        })
    }

    pub fn event(event: impl IntoLifecycleEvent) -> Result<Self, QueryBuildError> {
        Ok(Self {
            kind: LifecycleConditionKind::AllOf(LifecycleEvents::new(vec![
                event.into_lifecycle_event()?,
            ])?),
        })
    }
}

// ── LifecycleCompletion ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleCompletionKind {
    Configuration,
    AnySink(LifecycleSinks),
    AllSinks(LifecycleSinks),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleCompletion {
    kind: LifecycleCompletionKind,
}

/// Non-empty, bounded, deterministic lifecycle sink collections.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LifecycleSinks(CanonicalLifecycleItems<LifecycleSink>);

impl LifecycleSinks {
    fn new(sinks: Vec<LifecycleSink>) -> Result<Self, QueryBuildError> {
        Ok(Self(CanonicalLifecycleItems::new(
            sinks,
            QueryBuildError::EmptyLifecycleSinks,
            "lifecycle completion sinks",
            limits::MAX_LIFECYCLE_SINKS,
        )?))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LifecycleSink> {
        self.0.iter()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl LifecycleCompletion {
    pub(crate) fn kind(&self) -> &LifecycleCompletionKind {
        &self.kind
    }

    pub fn configuration() -> Self {
        Self {
            kind: LifecycleCompletionKind::Configuration,
        }
    }

    pub fn any_sink<I, S>(sinks: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: IntoLifecycleSink,
    {
        let sinks = bounded_lifecycle_items(
            sinks,
            "lifecycle completion sinks",
            IntoLifecycleSink::into_lifecycle_sink,
        )?;
        Ok(Self {
            kind: LifecycleCompletionKind::AnySink(LifecycleSinks::new(sinks)?),
        })
    }

    /// Require every sink for the same tracked object, in path order.
    ///
    /// Unlike [`Self::any_sink`], one matching sink does not complete the
    /// flow. Each sink is a separate bounded correlation event; unknown,
    /// escaped, reassigned, or incompatible-path objects cannot satisfy the
    /// conjunction.
    pub fn all_sinks<I, S>(sinks: I) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: IntoLifecycleSink,
    {
        let sinks = bounded_lifecycle_items(
            sinks,
            "lifecycle completion sinks",
            IntoLifecycleSink::into_lifecycle_sink,
        )?;
        Ok(Self {
            kind: LifecycleCompletionKind::AllSinks(LifecycleSinks::new(sinks)?),
        })
    }
}

// Fallible completion input accepted by lifecycle query builders.
define_lifecycle_adapter!(
    IntoLifecycleCompletion,
    into_lifecycle_completion,
    LifecycleCompletion
);

// ── LifecycleSink ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LifecycleSinkKind {
    ArgumentOf {
        endpoint: LifecycleCallEndpoint,
        index: usize,
    },
    AnyArgumentOf {
        endpoint: LifecycleCallEndpoint,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifecycleSink {
    pub(crate) kind: LifecycleSinkKind,
}

impl LifecycleSink {
    pub(crate) fn kind(&self) -> &LifecycleSinkKind {
        &self.kind
    }

    /// Sink argument of a strict configured global call, e.g. `fetch(value)`.
    pub fn argument_of_global(
        name: impl Into<String>,
        index: usize,
    ) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            name,
            |chain| LifecycleCallTarget::Global(chain.as_str().into()),
            Some(index),
        )
    }

    /// Sink argument of a rooted member call, e.g.
    /// `document.body.appendChild(value)`.
    pub fn argument_of_member(
        chain: impl Into<String>,
        index: usize,
    ) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            chain,
            |chain| LifecycleCallTarget::RootedMember(chain.path().clone()),
            Some(index),
        )
    }

    fn build_call_sink(
        chain: impl Into<String>,
        target: impl FnOnce(&MemberChain) -> LifecycleCallTarget,
        index: Option<usize>,
    ) -> Result<Self, QueryBuildError> {
        let chain = chain.into();
        if chain.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        let chain = checked_chain(chain)?;
        let target = target(&chain);
        let endpoint = LifecycleCallEndpoint::new(chain, target);
        let kind = match index {
            Some(index) => {
                if index > limits::MAX_ARGUMENT_INDEX {
                    return Err(QueryBuildError::InvalidArgumentIndex(index));
                }
                LifecycleSinkKind::ArgumentOf { endpoint, index }
            }
            None => LifecycleSinkKind::AnyArgumentOf { endpoint },
        };
        Ok(Self { kind })
    }

    /// Sink of any argument of a strict configured global call.
    pub fn any_argument_of_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            name,
            |chain| LifecycleCallTarget::Global(chain.as_str().into()),
            None,
        )
    }

    /// Sink of any argument of a rooted member call.
    pub fn any_argument_of_member(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        Self::build_call_sink(
            chain,
            |chain| LifecycleCallTarget::RootedMember(chain.path().clone()),
            None,
        )
    }

    pub fn chain(&self) -> &str {
        match &self.kind {
            LifecycleSinkKind::ArgumentOf { endpoint, .. }
            | LifecycleSinkKind::AnyArgumentOf { endpoint } => endpoint.chain(),
        }
    }
}

// Fallible sink input accepted by [`LifecycleCompletion::any_sink`].
define_lifecycle_adapter!(IntoLifecycleSink, into_lifecycle_sink, LifecycleSink);

define_lifecycle_adapter!(IntoLifecycleSource, into_lifecycle_source, EventQuery);

mod private {
    pub trait Sealed {}

    impl Sealed for super::LifecycleEventBuilder {}
}

// ── LifecycleQueryBuilder ─────────────────────────────────────────────

use crate::api::rule::query::LifecycleQuery;

#[derive(Debug, Clone)]
struct LifecycleStages {
    symbol: String,
    sources: Vec<EventQuery>,
    condition: Option<LifecycleCondition>,
    completion: Option<LifecycleCompletion>,
}

impl LifecycleStages {
    fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
        }
    }

    fn source(&mut self, source: EventQuery) {
        self.sources.push(source);
    }

    fn try_source<S: IntoLifecycleSource>(&mut self, source: S) -> Result<(), QueryBuildError> {
        self.sources.push(source.into_lifecycle_source()?);
        Ok(())
    }

    fn try_condition<C: IntoLifecycleCondition>(
        &mut self,
        condition: C,
    ) -> Result<(), QueryBuildError> {
        if self.condition.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("condition"));
        }
        self.condition = Some(condition.into_lifecycle_condition()?);
        Ok(())
    }

    fn try_completion<C: IntoLifecycleCompletion>(
        &mut self,
        completion: C,
    ) -> Result<(), QueryBuildError> {
        if self.completion.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("completion"));
        }
        self.completion = Some(completion.into_lifecycle_completion()?);
        Ok(())
    }

    fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let Self {
            symbol,
            sources,
            condition,
            completion,
        } = self;
        if symbol.trim().is_empty() {
            return Err(QueryBuildError::EmptyEvidenceSymbol);
        }
        if sources.is_empty() {
            return Err(QueryBuildError::MissingLifecycleSources);
        }
        if sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                sources.len(),
            ));
        }

        // Validate only relationships between lifecycle stages. Collection
        // invariants are established by LifecycleEvents and LifecycleSinks.
        if let Some(ref completion) = completion {
            match completion.kind() {
                LifecycleCompletionKind::AnySink(_) | LifecycleCompletionKind::AllSinks(_) => {}
                LifecycleCompletionKind::Configuration => {
                    if condition.is_none() {
                        return Err(QueryBuildError::MissingLifecycleCondition);
                    }
                }
            }
        } else {
            return Err(QueryBuildError::MissingLifecycleCompletion);
        }

        Ok(LifecycleQuery {
            symbol,
            sources,
            condition,
            completion,
        })
    }
}

#[derive(Debug, Clone)]
struct LifecycleBuilderState {
    stages: LifecycleStages,
    invalid_operation: FirstError<QueryBuildError>,
}

impl LifecycleBuilderState {
    fn new(symbol: impl Into<String>) -> Self {
        Self {
            stages: LifecycleStages::new(symbol),
            invalid_operation: FirstError::default(),
        }
    }

    fn record_operation(&mut self, result: Result<(), QueryBuildError>) {
        if let Err(error) = result {
            self.invalid_operation.record(error);
        }
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleQueryBuilder {
    state: LifecycleBuilderState,
}

impl LifecycleQueryBuilder {
    pub fn source(mut self, source: EventQuery) -> Self {
        self.state.stages.source(source);
        self
    }

    /// Add a lifecycle source and return construction errors immediately.
    pub fn try_source<S: IntoLifecycleSource>(
        mut self,
        source: S,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_source(source)?;
        Ok(self)
    }

    pub fn condition(mut self, condition: LifecycleCondition) -> Self {
        let result = self.state.stages.try_condition(condition);
        self.state.record_operation(result);
        self
    }

    /// Set the lifecycle condition and return construction errors immediately.
    pub fn try_condition(
        mut self,
        condition: impl IntoLifecycleCondition,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_condition(condition)?;
        Ok(self)
    }

    pub fn completion(mut self, completion: LifecycleCompletion) -> Self {
        let result = self.state.stages.try_completion(completion);
        self.state.record_operation(result);
        self
    }

    /// Set lifecycle completion and return construction errors immediately.
    pub fn try_completion<C: IntoLifecycleCompletion>(
        mut self,
        completion: C,
    ) -> Result<Self, QueryBuildError> {
        self.state.stages.try_completion(completion)?;
        Ok(self)
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let LifecycleBuilderState {
            stages,
            invalid_operation,
        } = self.state;
        if let Some(error) = invalid_operation.take() {
            return Err(error);
        }
        stages.build()
    }
}

define_lifecycle_adapter!(
    IntoLifecycleCondition,
    into_lifecycle_condition,
    LifecycleCondition
);

#[derive(Debug, Clone)]
pub struct CatalogLifecycleQueryBuilder {
    state: LifecycleBuilderState,
}

impl CatalogLifecycleQueryBuilder {
    fn record_error(&mut self, error: QueryBuildError) {
        self.state.invalid_operation.record(error);
    }

    fn record_operation(&mut self, result: Result<(), QueryBuildError>) {
        if let Err(error) = result {
            self.record_error(error);
        }
    }

    pub fn source<S: IntoLifecycleSource>(mut self, source: S) -> Self {
        let result = self.state.stages.try_source(source);
        self.record_operation(result);
        self
    }

    pub fn condition<C: IntoLifecycleCondition>(mut self, condition: C) -> Self {
        let result = self.state.stages.try_condition(condition);
        self.record_operation(result);
        self
    }

    pub fn completion<C: IntoLifecycleCompletion>(mut self, completion: C) -> Self {
        let result = self.state.stages.try_completion(completion);
        self.record_operation(result);
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        let LifecycleBuilderState {
            stages,
            invalid_operation,
        } = self.state;
        if let Some(error) = invalid_operation.take() {
            return Err(error);
        }
        stages.build()
    }
}

impl LifecycleQuery {
    pub fn builder(symbol: impl Into<String>) -> LifecycleQueryBuilder {
        LifecycleQueryBuilder {
            state: LifecycleBuilderState::new(symbol),
        }
    }

    pub fn catalog_builder(symbol: impl Into<String>) -> CatalogLifecycleQueryBuilder {
        CatalogLifecycleQueryBuilder {
            state: LifecycleBuilderState::new(symbol),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Result<EventQuery, QueryBuildError> {
        EventQuery::member_call_rooted("document.createElement")
    }

    #[test]
    fn explicit_completion_and_conditions_build() {
        let lc = LifecycleQuery::catalog_builder("input")
            .source(source())
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "type",
                ValueMatcher::static_string().try_equals("file").unwrap(),
            )))
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(lc.sources.len(), 1);
        assert!(lc.condition.is_some());
        assert!(lc.completion.is_some());
    }

    #[test]
    fn deferred_builder_reports_first_invalid_operation() {
        let condition = || {
            LifecycleCondition::event(LifecycleEvent::property_write(
                "value",
                ValueMatcher::static_string().try_equals("value").unwrap(),
            ))
        };
        let completion = || LifecycleCompletion::configuration();
        let error = LifecycleQuery::catalog_builder("input")
            .source(source())
            .condition(condition())
            .condition(condition())
            .completion(completion())
            .completion(completion())
            .build()
            .expect_err("duplicate condition should be retained");
        assert_eq!(error, QueryBuildError::DuplicateLifecycleStage("condition"));
    }

    #[test]
    fn immediate_builder_reports_duplicate_stages_at_build() {
        let condition = LifecycleCondition::event(LifecycleEvent::property_write(
            "value",
            ValueMatcher::any_value(),
        ))
        .unwrap();
        let error = LifecycleQuery::builder("input")
            .try_source(source())
            .unwrap()
            .condition(condition.clone())
            .condition(condition)
            .completion(LifecycleCompletion::configuration())
            .build()
            .expect_err("duplicate condition should be retained");
        assert_eq!(error, QueryBuildError::DuplicateLifecycleStage("condition"));
    }

    #[test]
    fn deferred_condition_accepts_a_prebuilt_value() {
        let condition = LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::any_value(),
        ))
        .unwrap();
        let lifecycle = LifecycleQuery::catalog_builder("input")
            .source(source())
            .condition(condition)
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap();
        assert!(lifecycle.condition.is_some());
    }

    #[test]
    fn empty_sources_fail() {
        let err = LifecycleQuery::catalog_builder("empty")
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("source"));
    }

    #[test]
    fn lifecycle_source_accepts_event_query() {
        let query = EventQuery::call_global("fetch").unwrap();
        let lifecycle = LifecycleQuery::catalog_builder("fetch result")
            .source(query)
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "url",
                ValueMatcher::any_value(),
            )))
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(
            lifecycle.sources(),
            &[EventQuery::call_global("fetch").unwrap()]
        );
    }

    #[test]
    fn order_independent_lifecycle_alternatives_are_canonical() {
        let src = LifecycleEvent::property_write("src", ValueMatcher::any_value()).unwrap();
        let href = LifecycleEvent::property_write("href", ValueMatcher::any_value()).unwrap();
        assert_eq!(
            LifecycleCondition::any_of([src.clone(), href.clone()]).unwrap(),
            LifecycleCondition::any_of([href, src]).unwrap()
        );

        let first = LifecycleSink::argument_of_member("sink", 0).unwrap();
        let second = LifecycleSink::any_argument_of_member("other").unwrap();
        assert_eq!(
            LifecycleCompletion::any_sink([first.clone(), second.clone()]).unwrap(),
            LifecycleCompletion::any_sink([second, first]).unwrap()
        );
    }

    #[test]
    fn all_of_conditions_are_canonical() {
        let first = LifecycleEvent::property_write("first", ValueMatcher::any_value()).unwrap();
        let second = LifecycleEvent::property_write("second", ValueMatcher::any_value()).unwrap();
        let a = LifecycleCondition::all_of([first.clone(), second.clone(), first.clone()]).unwrap();
        let b = LifecycleCondition::all_of([second, first]).unwrap();
        assert_eq!(a, b);
        assert!(matches!(a.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 2));
    }

    #[test]
    fn lifecycle_collections_enforce_their_bounds_at_construction() {
        let events = (0..=limits::MAX_LIFECYCLE_EVENTS)
            .map(|index| {
                LifecycleEvent::property_write(
                    format!("property-{index}"),
                    ValueMatcher::any_value(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(matches!(
            LifecycleCondition::all_of(events),
            Err(QueryBuildError::CollectionTooLarge(
                "lifecycle condition events",
                _
            ))
        ));

        let sinks = (0..=limits::MAX_LIFECYCLE_SINKS)
            .map(|index| LifecycleSink::argument_of_member(format!("sink-{index}"), 0))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(matches!(
            LifecycleCompletion::all_sinks(sinks),
            Err(QueryBuildError::CollectionTooLarge(
                "lifecycle completion sinks",
                _
            ))
        ));
    }

    #[test]
    fn all_sink_completion_is_bounded_and_deterministic() {
        let first = LifecycleSink::argument_of_member("document.head.appendChild", 0).unwrap();
        let second = LifecycleSink::argument_of_member("document.body.appendChild", 0).unwrap();
        let a =
            LifecycleCompletion::all_sinks([first.clone(), second.clone(), first.clone()]).unwrap();
        let b = LifecycleCompletion::all_sinks([second, first]).unwrap();
        assert_eq!(a, b);
        assert!(matches!(a.kind(), LifecycleCompletionKind::AllSinks(sinks) if sinks.len() == 2));
    }

    #[test]
    fn lifecycle_source_arg_adds_argument_constraint() {
        let s = EventQuery::member_call_rooted("foo.bar")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().try_equals("val").unwrap());
        let s = s.unwrap();
        assert_eq!(s.constraints().len(), 1);
        assert_eq!(s.constraints()[0].arg_index().get(), 0);
    }

    #[test]
    fn lifecycle_argument_limits_count_groups_and_per_group_predicates() {
        let mut source = EventQuery::member_call_rooted("foo.bar").unwrap();
        for index in 0..limits::MAX_ARGUMENT_GROUPS {
            source = source.with_arg(index, ValueMatcher::any_value()).unwrap();
        }
        assert_eq!(source.constraints().len(), limits::MAX_ARGUMENT_GROUPS);
        assert!(matches!(
            source.with_arg(limits::MAX_ARGUMENT_GROUPS, ValueMatcher::any_value()),
            Err(QueryBuildError::ExcessiveArgumentGroups(_))
        ));

        let mut event = LifecycleEvent::member_call("foo").unwrap();
        for _ in 0..limits::MAX_PREDICATES_PER_ARGUMENT {
            event = event.arg(0, ValueMatcher::any_value()).unwrap();
        }
        assert!(matches!(
            event.arg(0, ValueMatcher::any_value()),
            Err(QueryBuildError::ExcessivePredicates { index: 0, .. })
        ));

        let query = crate::api::rule::EventQuery::call_global("foo").unwrap();
        let mut query = query;
        for index in 0..limits::MAX_ARGUMENT_GROUPS {
            query = query.with_arg(index, ValueMatcher::any_value()).unwrap();
        }
        assert_eq!(query.constraints().len(), limits::MAX_ARGUMENT_GROUPS);
    }

    #[test]
    fn lifecycle_event_property_write_holds_property_and_value() {
        let value = ValueMatcher::any_value();
        let event = LifecycleEvent::property_write("src", value).unwrap();
        assert!(
            matches!(event.kind(), LifecycleEventKind::PropertyWrite { property, .. } if property == "src")
        );
    }

    #[test]
    fn lifecycle_event_member_call_builds_with_args() {
        let event: LifecycleEvent = LifecycleEvent::member_call("addEventListener")
            .unwrap()
            .arg(0, ValueMatcher::static_string().try_equals("load").unwrap())
            .unwrap()
            .build();
        assert!(
            matches!(event.kind(), LifecycleEventKind::MemberCall { member, .. } if member.as_str() == "addEventListener")
        );
    }

    #[test]
    fn lifecycle_event_text_and_argument_indices_are_checked() {
        assert!(matches!(
            LifecycleEvent::property_write("", ValueMatcher::any_value()),
            Err(QueryBuildError::EmptyIdentityName)
        ));
        assert!(matches!(
            LifecycleEvent::member_call(""),
            Err(QueryBuildError::EmptyIdentityName)
        ));
        assert!(matches!(
            LifecycleEvent::member_call("setAttribute")
                .unwrap()
                .arg(256, ValueMatcher::any_value()),
            Err(QueryBuildError::InvalidArgumentIndex(256))
        ));
    }

    #[test]
    fn lifecycle_condition_any_of_accepts_multiple_events() {
        let condition = LifecycleCondition::any_of([
            LifecycleEvent::property_write("a", ValueMatcher::any_value()),
            LifecycleEvent::property_write("b", ValueMatcher::any_value()),
        ])
        .unwrap();
        assert!(
            matches!(condition.kind(), LifecycleConditionKind::AnyOf(events) if events.len() == 2)
        );
    }

    #[test]
    fn lifecycle_condition_all_of_accepts_multiple_events() {
        let condition = LifecycleCondition::all_of([LifecycleEvent::property_write(
            "x",
            ValueMatcher::any_value(),
        )])
        .unwrap();
        assert!(
            matches!(condition.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 1)
        );
    }

    #[test]
    fn lifecycle_condition_event_wraps_in_all_of() {
        let condition = LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::static_string().try_equals("file").unwrap(),
        ))
        .unwrap();
        assert!(
            matches!(condition.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 1)
        );
    }

    #[test]
    fn lifecycle_completion_configuration_has_no_sinks() {
        let completion = LifecycleCompletion::configuration();
        assert!(matches!(
            completion.kind(),
            LifecycleCompletionKind::Configuration
        ));
    }

    #[test]
    fn lifecycle_completion_any_sink_holds_sink_matchers() {
        let sink = LifecycleSink::argument_of_member("target.appendChild", 0).unwrap();
        let completion = LifecycleCompletion::any_sink([sink]).unwrap();
        assert!(
            matches!(completion.kind(), LifecycleCompletionKind::AnySink(sinks) if sinks.len() == 1)
        );
    }

    #[test]
    fn lifecycle_sink_argument_of_holds_chain_and_index() {
        let sink = LifecycleSink::argument_of_member("parent.appendChild", 0).unwrap();
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            LifecycleSinkKind::ArgumentOf { index: 0, .. }
        ));
    }

    #[test]
    fn lifecycle_sink_any_argument_of_holds_chain() {
        let sink = LifecycleSink::any_argument_of_member("parent.appendChild").unwrap();
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            LifecycleSinkKind::AnyArgumentOf { .. }
        ));
    }

    #[test]
    fn configuration_completion_requires_condition() {
        let err = LifecycleQuery::catalog_builder("test")
            .source(source())
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap_err();
        assert!(
            err.to_string().contains("condition"),
            "configuration completion without condition: {err}"
        );
    }

    #[test]
    fn any_sink_requires_non_empty_sinks() {
        let err = LifecycleQuery::catalog_builder("test")
            .source(source())
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "x",
                ValueMatcher::any_value(),
            )))
            .completion(LifecycleCompletion::any_sink(Vec::<
                Result<LifecycleSink, QueryBuildError>,
            >::new()))
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("sink"), "empty any_sink: {err}");
    }

    #[test]
    fn completion_is_required() {
        let err = LifecycleQuery::catalog_builder("test")
            .source(source())
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "x",
                ValueMatcher::any_value(),
            )))
            .build()
            .unwrap_err();
        assert!(
            err.to_string().contains("completion"),
            "missing completion: {err}"
        );
    }

    #[test]
    fn empty_any_of_condition_fails() {
        let condition = LifecycleCondition::any_of::<[LifecycleEvent; 0]>([]);
        let err = LifecycleQuery::catalog_builder("test")
            .source(source())
            .condition(condition)
            .completion(LifecycleCompletion::any_sink([
                LifecycleSink::argument_of_member("target.appendChild", 0),
            ]))
            .build()
            .unwrap_err();
        assert!(
            err.to_string().contains("condition"),
            "empty any_of condition: {err}"
        );
    }

    #[test]
    fn empty_all_of_condition_fails() {
        let condition = LifecycleCondition::all_of::<[LifecycleEvent; 0]>([]);
        let err = LifecycleQuery::catalog_builder("test")
            .source(source())
            .condition(condition)
            .completion(LifecycleCompletion::any_sink([
                LifecycleSink::argument_of_member("target.appendChild", 0),
            ]))
            .build()
            .unwrap_err();
        assert!(
            err.to_string().contains("condition"),
            "empty all_of condition: {err}"
        );
    }

    #[test]
    fn try_source_reports_constructor_errors_at_the_call_site() {
        let error = LifecycleQuery::builder("test")
            .try_source(EventQuery::member_call_rooted(""))
            .unwrap_err();
        assert!(matches!(error, QueryBuildError::MalformedChain(_)));
    }
}
