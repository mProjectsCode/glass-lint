use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::query::value::{
    ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ValueMatcher,
};

// ── LifecycleSource ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleSource {
    chain: String,
    arguments: Vec<ArgumentConstraint>,
}

impl LifecycleSource {
    pub fn returned_by(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            arguments: Vec::new(),
        }
    }

    pub fn chain(&self) -> &str {
        &self.chain
    }

    pub fn arguments(&self) -> &[ArgumentConstraint] {
        &self.arguments
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn with_arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.arguments
            .push(ArgumentConstraint::new(arg_idx, matcher));
        self
    }

    #[must_use]
    pub fn arg(self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.with_arg(index, matcher)
    }
}

// ── LifecycleEvent ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LifecycleEventKind {
    PropertyWrite {
        property: SmolStr,
        value: ValueMatcher,
    },
    MemberCall {
        member: String,
        arguments: Vec<ArgumentConstraint>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleEvent {
    pub(crate) kind: LifecycleEventKind,
}

impl LifecycleEvent {
    pub(crate) fn kind(&self) -> &LifecycleEventKind {
        &self.kind
    }

    pub fn property_write(property: impl Into<SmolStr>, value: ValueMatcher) -> Self {
        Self {
            kind: LifecycleEventKind::PropertyWrite {
                property: property.into(),
                value,
            },
        }
    }

    pub fn member_call(member: impl Into<String>) -> LifecycleEventBuilder {
        LifecycleEventBuilder {
            event: LifecycleEventKind::MemberCall {
                member: member.into(),
                arguments: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleEventBuilder {
    event: LifecycleEventKind,
}

impl LifecycleEventBuilder {
    #[allow(clippy::cast_possible_truncation)]
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        if let LifecycleEventKind::MemberCall { arguments, .. } = &mut self.event {
            let arg_idx = ArgumentIndex::new_unchecked(index as u8);
            arguments.push(ArgumentConstraint::new(arg_idx, matcher));
        }
        self
    }

    pub fn build(self) -> LifecycleEvent {
        LifecycleEvent { kind: self.event }
    }
}

impl From<LifecycleEventBuilder> for LifecycleEvent {
    fn from(value: LifecycleEventBuilder) -> Self {
        value.build()
    }
}

// ── LifecycleCondition ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LifecycleConditionKind {
    AnyOf(Vec<LifecycleEvent>),
    AllOf(Vec<LifecycleEvent>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleCondition {
    pub(crate) kind: LifecycleConditionKind,
}

impl LifecycleCondition {
    pub(crate) fn kind(&self) -> &LifecycleConditionKind {
        &self.kind
    }

    pub fn any_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<LifecycleEvent>,
    {
        Self {
            kind: LifecycleConditionKind::AnyOf(events.into_iter().map(Into::into).collect()),
        }
    }

    pub fn all_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<LifecycleEvent>,
    {
        Self {
            kind: LifecycleConditionKind::AllOf(events.into_iter().map(Into::into).collect()),
        }
    }

    pub fn event(event: impl Into<LifecycleEvent>) -> Self {
        Self {
            kind: LifecycleConditionKind::AllOf(vec![event.into()]),
        }
    }
}

// ── LifecycleCompletion ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LifecycleCompletionKind {
    Configuration,
    AnySink(Vec<LifecycleSink>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleCompletion {
    pub(crate) kind: LifecycleCompletionKind,
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

    pub fn any_sink<I>(sinks: I) -> Self
    where
        I: IntoIterator<Item = LifecycleSink>,
    {
        Self {
            kind: LifecycleCompletionKind::AnySink(sinks.into_iter().collect()),
        }
    }
}

// ── LifecycleSink ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum LifecycleSinkKind {
    ArgumentOf { chain: String, index: usize },
    AnyArgumentOf { chain: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleSink {
    pub(crate) kind: LifecycleSinkKind,
}

impl LifecycleSink {
    pub(crate) fn kind(&self) -> &LifecycleSinkKind {
        &self.kind
    }

    pub fn argument_of(chain: impl Into<String>, index: usize) -> Self {
        Self {
            kind: LifecycleSinkKind::ArgumentOf {
                chain: chain.into(),
                index,
            },
        }
    }

    pub fn any_argument_of(chain: impl Into<String>) -> Self {
        Self {
            kind: LifecycleSinkKind::AnyArgumentOf {
                chain: chain.into(),
            },
        }
    }

    pub fn chain(&self) -> &str {
        match &self.kind {
            LifecycleSinkKind::ArgumentOf { chain, .. }
            | LifecycleSinkKind::AnyArgumentOf { chain } => chain,
        }
    }
}

// ── LifecycleQueryBuilder ─────────────────────────────────────────────

use crate::api::rule::query::{
    EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryBuildError, VarId, limits,
};

#[derive(Debug, Clone)]
pub struct LifecycleQueryBuilder {
    symbol: String,
    sources: Vec<EventQuery>,
    condition: Option<LifecycleCondition>,
    completion: Option<LifecycleCompletion>,
    invalid_operation: Option<&'static str>,
}

impl LifecycleQueryBuilder {
    #[allow(clippy::needless_pass_by_value)]
    pub fn source(mut self, source: LifecycleSource) -> Self {
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from(source.chain()),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from(source.chain()),
            },
            constraints: source.arguments().to_vec(),
        };
        self.sources.push(eq);
        self
    }

    pub fn condition(mut self, condition: LifecycleCondition) -> Self {
        if self.condition.is_some() {
            self.invalid_operation = Some("condition may only be specified once");
        } else {
            self.condition = Some(condition);
        }
        self
    }

    pub fn completion(mut self, completion: LifecycleCompletion) -> Self {
        if self.completion.is_some() {
            self.invalid_operation = Some("completion may only be specified once");
        } else {
            self.completion = Some(completion);
        }
        self
    }

    pub fn build(self) -> Result<LifecycleQuery, QueryBuildError> {
        if let Some(op) = self.invalid_operation {
            return Err(QueryBuildError::EmptyCollection(op));
        }
        if self.sources.is_empty() {
            return Err(QueryBuildError::EmptyCollection("lifecycle sources"));
        }
        if self.sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                self.sources.len(),
            ));
        }
        Ok(LifecycleQuery {
            symbol: self.symbol,
            sources: self.sources,
            condition: self.condition,
            completion: self.completion,
        })
    }
}

impl LifecycleQuery {
    pub fn builder(symbol: impl Into<String>) -> LifecycleQueryBuilder {
        LifecycleQueryBuilder {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
            invalid_operation: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> LifecycleSource {
        LifecycleSource::returned_by("document.createElement")
    }

    #[test]
    fn explicit_completion_and_conditions_build() {
        let lc = LifecycleQuery::builder("input")
            .source(source())
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "type",
                ValueMatcher::static_string().equals("file"),
            )))
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(lc.sources.len(), 1);
        assert!(lc.condition.is_some());
        assert!(lc.completion.is_some());
    }

    #[test]
    fn empty_sources_fail() {
        let err = LifecycleQuery::builder("empty")
            .completion(LifecycleCompletion::configuration())
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("sources"));
    }

    #[test]
    fn lifecycle_source_returned_by_has_expected_chain() {
        let s = LifecycleSource::returned_by("foo.bar");
        assert_eq!(s.chain(), "foo.bar");
        assert!(s.arguments().is_empty());
    }

    #[test]
    fn lifecycle_source_arg_adds_argument_constraint() {
        let s = LifecycleSource::returned_by("foo.bar")
            .arg(0, ValueMatcher::static_string().equals("val"));
        assert_eq!(s.arguments().len(), 1);
        assert_eq!(s.arguments()[0].index(), 0);
    }

    #[test]
    fn lifecycle_event_property_write_holds_property_and_value() {
        let value = ValueMatcher::any_value();
        let event = LifecycleEvent::property_write("src", value);
        assert!(
            matches!(event.kind(), LifecycleEventKind::PropertyWrite { property, .. } if property == "src")
        );
    }

    #[test]
    fn lifecycle_event_member_call_builds_with_args() {
        let event: LifecycleEvent = LifecycleEvent::member_call("addEventListener")
            .arg(0, ValueMatcher::static_string().equals("load"))
            .build();
        assert!(
            matches!(event.kind(), LifecycleEventKind::MemberCall { member, .. } if member == "addEventListener")
        );
    }

    #[test]
    fn lifecycle_condition_any_of_accepts_multiple_events() {
        let condition = LifecycleCondition::any_of([
            LifecycleEvent::property_write("a", ValueMatcher::any_value()),
            LifecycleEvent::property_write("b", ValueMatcher::any_value()),
        ]);
        assert!(
            matches!(condition.kind(), LifecycleConditionKind::AnyOf(events) if events.len() == 2)
        );
    }

    #[test]
    fn lifecycle_condition_all_of_accepts_multiple_events() {
        let condition = LifecycleCondition::all_of([LifecycleEvent::property_write(
            "x",
            ValueMatcher::any_value(),
        )]);
        assert!(
            matches!(condition.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 1)
        );
    }

    #[test]
    fn lifecycle_condition_event_wraps_in_all_of() {
        let condition = LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::static_string().equals("file"),
        ));
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
        let sink = LifecycleSink::argument_of("target.appendChild", 0);
        let completion = LifecycleCompletion::any_sink([sink]);
        assert!(
            matches!(completion.kind(), LifecycleCompletionKind::AnySink(sinks) if sinks.len() == 1)
        );
    }

    #[test]
    fn lifecycle_sink_argument_of_holds_chain_and_index() {
        let sink = LifecycleSink::argument_of("parent.appendChild", 0);
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            LifecycleSinkKind::ArgumentOf { index: 0, .. }
        ));
    }

    #[test]
    fn lifecycle_sink_any_argument_of_holds_chain() {
        let sink = LifecycleSink::any_argument_of("parent.appendChild");
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            LifecycleSinkKind::AnyArgumentOf { .. }
        ));
    }
}
