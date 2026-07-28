//! Argument predicates and declarative object-lifecycle flow matchers.
//!
//! Flow declarations describe a bounded source-to-configuration-to-completion
//! lifecycle. They become immutable predicates over semantic facts after
//! validation and compilation.

use smol_str::SmolStr;

use crate::api::rule::MatcherBuildError;

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueMatcher {
    /// Predicate family and payload.
    pub(crate) kind: ValueMatcherKind,
}

impl ValueMatcher {
    /// Borrow the value matcher kind.
    pub fn kind(&self) -> &ValueMatcherKind {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ValueMatcherKind {
    /// Accept any value, including unknown/dynamic values.
    Any,
    /// Require a proven static string predicate.
    StaticString(StaticStringPredicate),
}

/// Internal kind of a static-string predicate.
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

impl ValueMatcher {
    fn with_static_predicate(mut self, kind: StaticStringPredicateKind) -> Self {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::new(kind));
        self
    }

    /// Matches both proven static values and dynamic or unknown values.
    #[must_use]
    pub fn any_value() -> Self {
        Self {
            kind: ValueMatcherKind::Any,
        }
    }

    /// Starts a predicate that requires a proven static string.
    #[must_use]
    pub fn static_string() -> Self {
        let kind = StaticStringPredicateKind::Any;
        Self {
            kind: ValueMatcherKind::StaticString(StaticStringPredicate::new(kind)),
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
        self.with_static_predicate(StaticStringPredicateKind::Exact(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn starts_with_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicateKind::Prefix(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn contains_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicateKind::ContainsAny(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn contains_all<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicateKind::ContainsAll(
            values.into_iter().map(Into::into).collect(),
        ))
    }
}

// ── ArgumentMatcher ──────────────────────────────────────────────────────

/// Internal kind of an argument predicate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ArgumentMatcherKind {
    /// Apply a value predicate.
    Value(ValueMatcher),
    /// Require a static object shape to contain these keys.
    ObjectKeys(Vec<String>),
    /// Require rooted expression identities from the argument object.
    RootedExpressions(Vec<String>),
    /// Require a proven static string in a named direct object property.
    ObjectPropertyValue {
        property: String,
        value: ValueMatcher,
    },
}

/// A predicate applied to one selected call argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArgumentMatcher {
    pub(crate) kind: ArgumentMatcherKind,
}

impl ArgumentMatcher {
    /// Borrow the argument matcher kind.
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
    /// Zero-based argument position.
    index: usize,
    /// Predicate required at that position.
    matcher: ArgumentMatcher,
}

impl ArgumentConstraint {
    pub fn new(index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        Self {
            index,
            matcher: matcher.into(),
        }
    }

    /// Return the zero-based argument position.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Borrow the argument predicate.
    pub fn matcher(&self) -> &ArgumentMatcher {
        &self.matcher
    }
}

/// A call that returns the object tracked by an [`ObjectFlowMatcher`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectSourceMatcher {
    /// Rooted source chain.
    chain: String,
    /// Argument constraints on the source call.
    arguments: Vec<ArgumentConstraint>,
}

impl ObjectSourceMatcher {
    #[must_use]
    pub fn returned_by(chain: impl Into<String>) -> Self {
        Self {
            chain: chain.into(),
            arguments: Vec::new(),
        }
    }

    /// Borrow the rooted source chain.
    pub fn chain(&self) -> &str {
        &self.chain
    }

    /// Borrow the source call argument constraints.
    pub fn arguments(&self) -> &[ArgumentConstraint] {
        &self.arguments
    }

    #[must_use]
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.arguments.push(ArgumentConstraint::new(index, matcher));
        self
    }
}

// ── ObjectEventMatcher ───────────────────────────────────────────────────

/// Internal kind of a lifecycle event on a tracked object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ObjectEventMatcherKind {
    PropertyWrite {
        /// Written property name.
        property: SmolStr,
        /// Required value predicate.
        value: ValueMatcher,
    },
    MemberCall {
        /// Called member name.
        member: String,
        /// Argument predicates for the call.
        arguments: Vec<ArgumentConstraint>,
    },
}

/// A lifecycle event observed on a tracked object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectEventMatcher {
    pub(crate) kind: ObjectEventMatcherKind,
}

impl ObjectEventMatcher {
    /// Borrow the event kind.
    pub(crate) fn kind(&self) -> &ObjectEventMatcherKind {
        &self.kind
    }

    pub fn property_write(property: impl Into<SmolStr>, value: ValueMatcher) -> Self {
        Self {
            kind: ObjectEventMatcherKind::PropertyWrite {
                property: property.into(),
                value,
            },
        }
    }

    pub fn member_call(member: impl Into<String>) -> ObjectEventBuilder {
        ObjectEventBuilder {
            event: ObjectEventMatcherKind::MemberCall {
                member: member.into(),
                arguments: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectEventBuilder {
    /// Partially constructed lifecycle event.
    event: ObjectEventMatcherKind,
}

impl ObjectEventBuilder {
    #[must_use]
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        if let ObjectEventMatcherKind::MemberCall { arguments, .. } = &mut self.event {
            arguments.push(ArgumentConstraint::new(index, matcher));
        }
        self
    }

    pub fn build(self) -> ObjectEventMatcher {
        ObjectEventMatcher { kind: self.event }
    }
}

impl From<ObjectEventBuilder> for ObjectEventMatcher {
    fn from(value: ObjectEventBuilder) -> Self {
        value.build()
    }
}

// ── FlowCondition ────────────────────────────────────────────────────────

/// Internal kind of a flow configuration condition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FlowConditionKind {
    /// At least one event must be observed.
    AnyOf(Vec<ObjectEventMatcher>),
    /// Every event must be observed.
    AllOf(Vec<ObjectEventMatcher>),
}

/// Explicitly combines the events that configure a tracked object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowCondition {
    pub(crate) kind: FlowConditionKind,
}

impl FlowCondition {
    /// Borrow the flow condition kind.
    pub(crate) fn kind(&self) -> &FlowConditionKind {
        &self.kind
    }

    pub fn any_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<ObjectEventMatcher>,
    {
        Self {
            kind: FlowConditionKind::AnyOf(events.into_iter().map(Into::into).collect()),
        }
    }

    pub fn all_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<ObjectEventMatcher>,
    {
        Self {
            kind: FlowConditionKind::AllOf(events.into_iter().map(Into::into).collect()),
        }
    }

    pub fn event(event: impl Into<ObjectEventMatcher>) -> Self {
        Self {
            kind: FlowConditionKind::AllOf(vec![event.into()]),
        }
    }
}

// ── FlowCompletion ───────────────────────────────────────────────────────

/// Internal kind of a flow completion mode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FlowCompletionKind {
    /// Emit once the configuration condition is satisfied.
    Configuration,
    /// Emit when any configured sink receives the tracked object.
    AnySink(Vec<FlowSinkMatcher>),
}

/// The point at which a configured object produces evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowCompletion {
    pub(crate) kind: FlowCompletionKind,
}

impl FlowCompletion {
    /// Borrow the completion kind.
    pub(crate) fn kind(&self) -> &FlowCompletionKind {
        &self.kind
    }

    #[must_use]
    pub fn configuration() -> Self {
        Self {
            kind: FlowCompletionKind::Configuration,
        }
    }

    pub fn any_sink<I>(sinks: I) -> Self
    where
        I: IntoIterator<Item = FlowSinkMatcher>,
    {
        Self {
            kind: FlowCompletionKind::AnySink(sinks.into_iter().collect()),
        }
    }
}

// ── FlowSinkMatcher ──────────────────────────────────────────────────────

/// Internal kind of a tracked-object sink matcher.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FlowSinkMatcherKind {
    /// Match one specific argument position.
    ArgumentOf { chain: String, index: usize },
    /// Match any argument position at the sink call.
    AnyArgumentOf { chain: String },
}

/// A tracked object appearing in a selected argument of a call.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowSinkMatcher {
    pub(crate) kind: FlowSinkMatcherKind,
}

impl FlowSinkMatcher {
    /// Borrow the sink matcher kind.
    pub(crate) fn kind(&self) -> &FlowSinkMatcherKind {
        &self.kind
    }

    #[must_use]
    pub fn argument_of(chain: impl Into<String>, index: usize) -> Self {
        Self {
            kind: FlowSinkMatcherKind::ArgumentOf {
                chain: chain.into(),
                index,
            },
        }
    }

    #[must_use]
    pub fn any_argument_of(chain: impl Into<String>) -> Self {
        Self {
            kind: FlowSinkMatcherKind::AnyArgumentOf {
                chain: chain.into(),
            },
        }
    }

    /// Return the rooted chain.
    pub fn chain(&self) -> &str {
        match &self.kind {
            FlowSinkMatcherKind::ArgumentOf { chain, .. }
            | FlowSinkMatcherKind::AnyArgumentOf { chain } => chain,
        }
    }
}

/// Declarative object lifecycle matching: source, configuration, completion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectFlowMatcher {
    /// Evidence symbol for the completed flow.
    symbol: String,
    /// Calls that produce tracked objects.
    sources: Vec<ObjectSourceMatcher>,
    /// Configuration condition.
    condition: Option<FlowCondition>,
    /// Completion/emission mode.
    completion: Option<FlowCompletion>,
}

impl ObjectFlowMatcher {
    /// Start a builder for a named object flow.
    pub fn builder(symbol: impl Into<String>) -> ObjectFlowMatcherBuilder {
        ObjectFlowMatcherBuilder {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
            invalid_operation: None,
        }
    }

    /// Validate that the matcher is complete and well-formed.
    pub(crate) fn validate(&self) -> Result<(), MatcherBuildError> {
        if self.sources.is_empty() {
            return Err(MatcherBuildError::MissingFlowSource);
        }
        if self.completion.is_none() {
            return Err(MatcherBuildError::MissingFlowCompletion);
        }
        Ok(())
    }

    /// Borrow the evidence symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Borrow the object-producing sources.
    pub fn sources(&self) -> &[ObjectSourceMatcher] {
        &self.sources
    }

    /// Borrow the optional configuration condition.
    pub fn condition(&self) -> Option<&FlowCondition> {
        self.condition.as_ref()
    }

    /// Borrow the optional completion mode.
    pub fn completion(&self) -> Option<&FlowCompletion> {
        self.completion.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ObjectFlowMatcherBuilder {
    /// Flow evidence symbol under construction.
    symbol: String,
    /// Object-producing sources.
    sources: Vec<ObjectSourceMatcher>,
    /// Optional configuration condition.
    condition: Option<FlowCondition>,
    /// Optional completion mode.
    completion: Option<FlowCompletion>,
    /// First duplicate-operation error, retained for deterministic reporting.
    invalid_operation: Option<&'static str>,
}

impl ObjectFlowMatcherBuilder {
    #[must_use]
    /// Add one object-producing source.
    pub fn source(mut self, source: ObjectSourceMatcher) -> Self {
        self.sources.push(source);
        self
    }

    #[must_use]
    /// Set the configuration condition exactly once.
    pub fn configured_by(mut self, condition: FlowCondition) -> Self {
        if self.condition.is_some() {
            self.invalid_operation = Some("configured_by may only be specified once");
        } else {
            self.condition = Some(condition);
        }
        self
    }

    #[must_use]
    /// Set the completion mode exactly once.
    pub fn complete_at(mut self, completion: FlowCompletion) -> Self {
        if self.completion.is_some() {
            self.invalid_operation = Some("complete_at may only be specified once");
        } else {
            self.completion = Some(completion);
        }
        self
    }

    /// Build and validate the object-flow matcher.
    pub fn build(self) -> Result<ObjectFlowMatcher, MatcherBuildError> {
        if let Some(op) = self.invalid_operation {
            return Err(MatcherBuildError::DuplicateFlowOperation(op));
        }
        let matcher = ObjectFlowMatcher {
            symbol: self.symbol,
            sources: self.sources,
            condition: self.condition,
            completion: self.completion,
        };
        matcher.validate()?;
        Ok(matcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ObjectSourceMatcher {
        ObjectSourceMatcher::returned_by("document.createElement")
    }

    #[test]
    fn explicit_completion_and_conditions_build() {
        let matcher = ObjectFlowMatcher::builder("input")
            .source(source())
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "type",
                ValueMatcher::static_string().equals("file"),
            )))
            .complete_at(FlowCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(matcher.symbol(), "input");
    }

    #[test]
    fn empty_alternatives_and_duplicate_operations_fail() {
        let empty = ObjectFlowMatcher::builder("empty")
            .source(source())
            .configured_by(FlowCondition::any_of(Vec::<ObjectEventMatcher>::new()))
            .complete_at(FlowCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(empty.symbol(), "empty");

        let err = ObjectFlowMatcher::builder("no_source")
            .complete_at(FlowCompletion::configuration())
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("source"));

        let err = ObjectFlowMatcher::builder("no_completion")
            .source(source())
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("completion"));

        let err = ObjectFlowMatcher::builder("duplicate")
            .source(source())
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "ready",
                ValueMatcher::any_value(),
            )))
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "again",
                ValueMatcher::any_value(),
            )))
            .complete_at(FlowCompletion::configuration())
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("once"));
    }

    // ── ValueMatcher tests ────────────────────────────────────────

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
                StaticStringPredicateKind::ContainsAny(vec!["token".into(), "secret".into()])
            ))
        );
    }

    #[test]
    fn value_matcher_contains_all_creates_contains_all() {
        let m = ValueMatcher::static_string().contains_all(["required", "field"]);
        assert_eq!(
            m.kind(),
            &ValueMatcherKind::StaticString(StaticStringPredicate::new(
                StaticStringPredicateKind::ContainsAll(vec!["required".into(), "field".into()])
            ))
        );
    }

    // ── StaticStringPredicate tests ───────────────────────────────

    #[test]
    fn static_string_predicate_new_round_trips_kind() {
        let p = StaticStringPredicate::new(StaticStringPredicateKind::Any);
        assert!(matches!(p.kind, StaticStringPredicateKind::Any));
    }

    // ── ArgumentMatcher tests ─────────────────────────────────────

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

    // ── ArgumentConstraint tests ──────────────────────────────────

    #[test]
    fn argument_constraint_new_holds_index_and_matcher() {
        let m = ArgumentMatcher::object_keys(["k"]);
        let c = ArgumentConstraint::new(2, m);
        assert_eq!(c.index(), 2);
        assert!(matches!(
            c.matcher().kind(),
            ArgumentMatcherKind::ObjectKeys(_)
        ));
    }

    // ── ObjectSourceMatcher tests ─────────────────────────────────

    #[test]
    fn object_source_matcher_returned_by_has_expected_chain() {
        let m = ObjectSourceMatcher::returned_by("foo.bar");
        assert_eq!(m.chain(), "foo.bar");
        assert!(m.arguments().is_empty());
    }

    #[test]
    fn object_source_matcher_arg_adds_argument_constraint() {
        let m = ObjectSourceMatcher::returned_by("foo.bar")
            .arg(0, ValueMatcher::static_string().equals("val"));
        assert_eq!(m.arguments().len(), 1);
        assert_eq!(m.arguments()[0].index(), 0);
    }

    // ── ObjectEventMatcher / ObjectEventBuilder tests ─────────────

    #[test]
    fn object_event_property_write_holds_property_and_value() {
        let value = ValueMatcher::any_value();
        let event = ObjectEventMatcher::property_write("src", value);
        assert!(
            matches!(event.kind(), ObjectEventMatcherKind::PropertyWrite { property, .. } if property == "src")
        );
    }

    #[test]
    fn object_event_member_call_builds_with_args() {
        let event: ObjectEventMatcher = ObjectEventMatcher::member_call("addEventListener")
            .arg(0, ValueMatcher::static_string().equals("load"))
            .build();
        assert!(
            matches!(event.kind(), ObjectEventMatcherKind::MemberCall { member, .. } if member == "addEventListener")
        );
    }

    // ── FlowCondition tests ───────────────────────────────────────

    #[test]
    fn flow_condition_any_of_accepts_multiple_events() {
        let condition = FlowCondition::any_of([
            ObjectEventMatcher::property_write("a", ValueMatcher::any_value()),
            ObjectEventMatcher::property_write("b", ValueMatcher::any_value()),
        ]);
        assert!(matches!(condition.kind(), FlowConditionKind::AnyOf(events) if events.len() == 2));
    }

    #[test]
    fn flow_condition_all_of_accepts_multiple_events() {
        let condition = FlowCondition::all_of([ObjectEventMatcher::property_write(
            "x",
            ValueMatcher::any_value(),
        )]);
        assert!(matches!(condition.kind(), FlowConditionKind::AllOf(events) if events.len() == 1));
    }

    #[test]
    fn flow_condition_event_wraps_in_all_of() {
        let condition = FlowCondition::event(ObjectEventMatcher::property_write(
            "type",
            ValueMatcher::static_string().equals("file"),
        ));
        assert!(matches!(condition.kind(), FlowConditionKind::AllOf(events) if events.len() == 1));
    }

    // ── FlowCompletion tests ──────────────────────────────────────

    #[test]
    fn flow_completion_configuration_has_no_sinks() {
        let completion = FlowCompletion::configuration();
        assert!(matches!(
            completion.kind(),
            FlowCompletionKind::Configuration
        ));
    }

    #[test]
    fn flow_completion_any_sink_holds_sink_matchers() {
        let sink = FlowSinkMatcher::argument_of("target.appendChild", 0);
        let completion = FlowCompletion::any_sink([sink]);
        assert!(
            matches!(completion.kind(), FlowCompletionKind::AnySink(sinks) if sinks.len() == 1)
        );
    }

    // ── FlowSinkMatcher tests ─────────────────────────────────────

    #[test]
    fn flow_sink_matcher_argument_of_holds_chain_and_index() {
        let sink = FlowSinkMatcher::argument_of("parent.appendChild", 0);
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            FlowSinkMatcherKind::ArgumentOf { index: 0, .. }
        ));
    }

    #[test]
    fn flow_sink_matcher_any_argument_of_holds_chain() {
        let sink = FlowSinkMatcher::any_argument_of("parent.appendChild");
        assert_eq!(sink.chain(), "parent.appendChild");
        assert!(matches!(
            sink.kind(),
            FlowSinkMatcherKind::AnyArgumentOf { .. }
        ));
    }

    // ── ObjectFlowMatcher accessors ───────────────────────────────

    #[test]
    fn object_flow_matcher_accessors_return_configured_values() {
        let source = ObjectSourceMatcher::returned_by("create");
        let matcher = ObjectFlowMatcher::builder("test")
            .source(source)
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "ready",
                ValueMatcher::any_value(),
            )))
            .complete_at(FlowCompletion::configuration())
            .build()
            .unwrap();
        assert_eq!(matcher.symbol(), "test");
        assert_eq!(matcher.sources().len(), 1);
        assert!(matcher.condition().is_some());
        assert!(matcher.completion().is_some());
    }
}
