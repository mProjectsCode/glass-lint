use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::{
    FirstError,
    query::{
        EventQuery, MemberChain, QueryBuildError, checked_chain, limits,
        value::{ArgumentConstraint, ArgumentConstraintsBuilder, ArgumentMatcher, ValueMatcher},
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
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleEventBuilder {
    event: LifecycleEventKind,
}

impl LifecycleEventBuilder {
    pub fn arg(
        mut self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        let index = super::value::ArgumentIndex::try_from_usize(index)?;
        if let LifecycleEventKind::MemberCall { arguments, .. } = &mut self.event {
            let mut builder = ArgumentConstraintsBuilder::from_constraints(arguments)?;
            builder.push(index, matcher)?;
            *arguments = builder.finish();
        }
        Ok(self)
    }

    pub fn build(self) -> LifecycleEvent {
        LifecycleEvent { kind: self.event }
    }
}

/// Fallible event input accepted by lifecycle condition constructors.
pub trait IntoLifecycleEvent {
    fn into_lifecycle_event(self) -> Result<LifecycleEvent, QueryBuildError>;
}

impl IntoLifecycleEvent for LifecycleEvent {
    fn into_lifecycle_event(self) -> Result<LifecycleEvent, QueryBuildError> {
        Ok(self)
    }
}

impl IntoLifecycleEvent for Result<LifecycleEvent, QueryBuildError> {
    fn into_lifecycle_event(self) -> Result<LifecycleEvent, QueryBuildError> {
        self
    }
}

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
pub(crate) struct LifecycleEvents(Box<[LifecycleEvent]>);

impl LifecycleEvents {
    fn new(mut events: Vec<LifecycleEvent>) -> Result<Self, QueryBuildError> {
        if events.is_empty() {
            return Err(QueryBuildError::EmptyLifecycleCondition);
        }
        events.sort();
        events.dedup();
        if events.len() > limits::MAX_LIFECYCLE_EVENTS {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle condition events",
                events.len(),
            ));
        }
        Ok(Self(events.into_boxed_slice()))
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LifecycleEvent> {
        self.0.iter()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
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
        let events = events
            .into_iter()
            .map(IntoLifecycleEvent::into_lifecycle_event)
            .collect::<Result<Vec<_>, _>>()?;
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
        let events = events
            .into_iter()
            .map(IntoLifecycleEvent::into_lifecycle_event)
            .collect::<Result<Vec<_>, _>>()?;
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
pub(crate) struct LifecycleSinks(Box<[LifecycleSink]>);

impl LifecycleSinks {
    fn new(mut sinks: Vec<LifecycleSink>) -> Result<Self, QueryBuildError> {
        if sinks.is_empty() {
            return Err(QueryBuildError::EmptyLifecycleSinks);
        }
        sinks.sort();
        sinks.dedup();
        if sinks.len() > limits::MAX_LIFECYCLE_SINKS {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle completion sinks",
                sinks.len(),
            ));
        }
        Ok(Self(sinks.into_boxed_slice()))
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
        let sinks = sinks
            .into_iter()
            .map(IntoLifecycleSink::into_lifecycle_sink)
            .collect::<Result<Vec<_>, _>>()?;
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
        let sinks = sinks
            .into_iter()
            .map(IntoLifecycleSink::into_lifecycle_sink)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            kind: LifecycleCompletionKind::AllSinks(LifecycleSinks::new(sinks)?),
        })
    }
}

macro_rules! define_lifecycle_adapter {
    ($trait_name:ident, $method:ident, $value:ty) => {
        pub trait $trait_name {
            fn $method(self) -> Result<$value, QueryBuildError>;
        }

        impl $trait_name for $value {
            fn $method(self) -> Result<$value, QueryBuildError> {
                Ok(self)
            }
        }

        impl $trait_name for Result<$value, QueryBuildError> {
            fn $method(self) -> Result<$value, QueryBuildError> {
                self
            }
        }
    };
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

/// Fallible source input accepted by the lifecycle query builder's `source`
/// method.
///
/// Sources deliberately accept the existing leaf [`EventQuery`] rather than
/// a full `QueryDecl`. The compiler later restricts the event to a global
/// call or rooted member call because only those forms produce a tracked
/// returned object in the flow engine.
pub trait IntoLifecycleSource: private::Sealed {
    fn into_lifecycle_source(self) -> Result<EventQuery, QueryBuildError>;
}

impl IntoLifecycleSource for EventQuery {
    fn into_lifecycle_source(self) -> Result<EventQuery, QueryBuildError> {
        Ok(self)
    }
}

impl IntoLifecycleSource for Result<EventQuery, QueryBuildError> {
    fn into_lifecycle_source(self) -> Result<EventQuery, QueryBuildError> {
        self
    }
}

mod private {
    pub trait Sealed {}

    impl Sealed for super::EventQuery {}
    impl Sealed for Result<super::EventQuery, super::QueryBuildError> {}
}

// ── LifecycleQueryBuilder ─────────────────────────────────────────────

use crate::api::rule::query::LifecycleQuery;

#[derive(Debug, Clone)]
pub struct LifecycleQueryBuilder {
    symbol: String,
    sources: Vec<EventQuery>,
    condition: Option<LifecycleCondition>,
    completion: Option<LifecycleCompletion>,
}

impl LifecycleQueryBuilder {
    pub fn source(mut self, source: EventQuery) -> Self {
        self.sources.push(source);
        self
    }

    /// Add a lifecycle source and return construction errors immediately.
    pub fn try_source<S: IntoLifecycleSource>(
        mut self,
        source: S,
    ) -> Result<Self, QueryBuildError> {
        self.sources.push(source.into_lifecycle_source()?);
        Ok(self)
    }

    pub fn condition(mut self, condition: LifecycleCondition) -> Self {
        if self.condition.is_none() {
            self.condition = Some(condition);
        }
        self
    }

    /// Set the lifecycle condition and return construction errors immediately.
    pub fn try_condition(
        mut self,
        condition: impl IntoLifecycleCondition,
    ) -> Result<Self, QueryBuildError> {
        if self.condition.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("condition"));
        }
        self.condition = Some(condition.into_lifecycle_condition()?);
        Ok(self)
    }

    pub fn completion(mut self, completion: LifecycleCompletion) -> Self {
        if self.completion.is_none() {
            self.completion = Some(completion);
        }
        self
    }

    /// Set lifecycle completion and return construction errors immediately.
    pub fn try_completion<C: IntoLifecycleCompletion>(
        mut self,
        completion: C,
    ) -> Result<Self, QueryBuildError> {
        if self.completion.is_some() {
            return Err(QueryBuildError::DuplicateLifecycleStage("completion"));
        }
        self.completion = Some(completion.into_lifecycle_completion()?);
        Ok(self)
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        if self.symbol.trim().is_empty() {
            return Err(QueryBuildError::EmptyEvidenceSymbol);
        }
        if self.sources.is_empty() {
            return Err(QueryBuildError::MissingLifecycleSources);
        }
        if self.sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                self.sources.len(),
            ));
        }

        // Validate only relationships between lifecycle stages. Collection
        // invariants are established by LifecycleEvents and LifecycleSinks.
        if let Some(ref completion) = self.completion {
            match completion.kind() {
                LifecycleCompletionKind::AnySink(_) | LifecycleCompletionKind::AllSinks(_) => {}
                LifecycleCompletionKind::Configuration => {
                    if self.condition.is_none() {
                        return Err(QueryBuildError::MissingLifecycleCondition);
                    }
                }
            }
        } else {
            return Err(QueryBuildError::MissingLifecycleCompletion);
        }

        Ok(LifecycleQuery {
            symbol: self.symbol,
            sources: self.sources,
            condition: self.condition,
            completion: self.completion,
        })
    }
}

pub trait IntoLifecycleCondition {
    fn into_lifecycle_condition(self) -> Result<LifecycleCondition, QueryBuildError>;
}

impl IntoLifecycleCondition for LifecycleCondition {
    fn into_lifecycle_condition(self) -> Result<LifecycleCondition, QueryBuildError> {
        Ok(self)
    }
}

impl IntoLifecycleCondition for Result<LifecycleCondition, QueryBuildError> {
    fn into_lifecycle_condition(self) -> Result<LifecycleCondition, QueryBuildError> {
        self
    }
}

#[derive(Debug, Clone)]
pub struct CatalogLifecycleQueryBuilder {
    inner: LifecycleQueryBuilder,
    invalid_operation: FirstError<QueryBuildError>,
}

impl CatalogLifecycleQueryBuilder {
    fn record_error(&mut self, error: QueryBuildError) {
        self.invalid_operation.record(error);
    }

    pub fn source<S: IntoLifecycleSource>(mut self, source: S) -> Self {
        match source.into_lifecycle_source() {
            Ok(source) => self.inner = self.inner.source(source),
            Err(error) => self.record_error(error),
        }
        self
    }

    pub fn condition(mut self, condition: Result<LifecycleCondition, QueryBuildError>) -> Self {
        match condition {
            Ok(condition) if self.inner.condition.is_none() => {
                self.inner = self.inner.condition(condition);
            }
            Ok(_) => self.record_error(QueryBuildError::DuplicateLifecycleStage("condition")),
            Err(error) => self.record_error(error),
        }
        self
    }

    pub fn completion<C: IntoLifecycleCompletion>(mut self, completion: C) -> Self {
        match completion.into_lifecycle_completion() {
            Ok(completion) if self.inner.completion.is_none() => {
                self.inner = self.inner.completion(completion);
            }
            Ok(_) => self.record_error(QueryBuildError::DuplicateLifecycleStage("completion")),
            Err(error) => self.record_error(error),
        }
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        if let Some(error) = self.invalid_operation.take() {
            return Err(error);
        }
        self.inner.build()
    }
}

impl LifecycleQuery {
    pub fn builder(symbol: impl Into<String>) -> LifecycleQueryBuilder {
        LifecycleQueryBuilder {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
        }
    }

    pub fn catalog_builder(symbol: impl Into<String>) -> CatalogLifecycleQueryBuilder {
        CatalogLifecycleQueryBuilder {
            inner: Self::builder(symbol),
            invalid_operation: FirstError::default(),
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
        assert_eq!(s.constraints()[0].index(), 0);
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
