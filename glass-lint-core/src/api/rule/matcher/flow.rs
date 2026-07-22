//! Argument predicates and declarative object-lifecycle flow matchers.
//!
//! Flow declarations describe a bounded source-to-configuration-to-completion
//! lifecycle. They become immutable predicates over semantic facts after
//! validation and compilation.

use smol_str::{SmolStr, ToSmolStr};

use crate::api::rule::matcher::MemberCallMatcher;

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValueMatcher {
    /// Predicate family and payload.
    pub(crate) kind: ValueMatcherKind,
}

impl ValueMatcher {
    /// Borrow the value matcher kind.
    pub fn kind(&self) -> &ValueMatcherKind {
        &self.kind
    }

    pub(crate) fn normalize(&mut self) {
        if let ValueMatcherKind::StaticString(predicate) = &mut self.kind {
            match &mut predicate.kind {
                StaticStringPredicateKind::Any => {}
                StaticStringPredicateKind::Exact(values)
                | StaticStringPredicateKind::Prefix(values)
                | StaticStringPredicateKind::ContainsAny(values)
                | StaticStringPredicateKind::ContainsAll(values) => {
                    crate::api::rule::matcher::normalize_strings(values);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueMatcherKind {
    /// Accept any value, including unknown/dynamic values.
    Any,
    /// Require a proven static string predicate.
    StaticString(StaticStringPredicate),
}

/// Internal kind of a static-string predicate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StaticStringPredicateKind {
    Any,
    Exact(Vec<String>),
    Prefix(Vec<String>),
    ContainsAny(Vec<String>),
    ContainsAll(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

    pub(crate) fn normalize(&mut self) {
        match &mut self.kind {
            ArgumentMatcherKind::Value(value) => value.normalize(),
            ArgumentMatcherKind::ObjectKeys(keys)
            | ArgumentMatcherKind::RootedExpressions(keys) => {
                crate::api::rule::matcher::normalize_strings(keys);
            }
            ArgumentMatcherKind::ObjectPropertyValue { property, value } => {
                *property = property.trim().to_string();
                value.normalize();
            }
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

    pub fn normalize_all(arguments: &mut Vec<Self>) {
        for argument in arguments.iter_mut() {
            argument.matcher.normalize();
        }
        arguments.sort_by_key(|argument| argument.index);
        arguments.dedup();
    }
}

/// A call that returns the object tracked by an [`ObjectFlowMatcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSourceMatcher {
    /// Source call identity and argument constraints.
    call: MemberCallMatcher,
}

impl ObjectSourceMatcher {
    #[must_use]
    pub fn returned_by(call: MemberCallMatcher) -> Self {
        Self { call }
    }

    /// Borrow the source call matcher.
    pub fn call(&self) -> &MemberCallMatcher {
        &self.call
    }
}

// ── ObjectEventMatcher ───────────────────────────────────────────────────

/// Internal kind of a lifecycle event on a tracked object.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub(crate) fn normalize(&mut self) {
        match &mut self.kind {
            ObjectEventMatcherKind::PropertyWrite { property, value } => {
                *property = property.trim().to_smolstr();
                value.normalize();
            }
            ObjectEventMatcherKind::MemberCall { member, arguments } => {
                *member = member.trim().to_string();
                ArgumentConstraint::normalize_all(arguments);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlowConditionKind {
    /// At least one event must be observed.
    AnyOf(Vec<ObjectEventMatcher>),
    /// Every event must be observed.
    AllOf(Vec<ObjectEventMatcher>),
}

/// Explicitly combines the events that configure a tracked object.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub(crate) fn normalize(&mut self) {
        let events = match &mut self.kind {
            FlowConditionKind::AnyOf(events) | FlowConditionKind::AllOf(events) => events,
        };
        for event in events {
            event.normalize();
        }
    }
}

// ── FlowCompletion ───────────────────────────────────────────────────────

/// Internal kind of a flow completion mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlowCompletionKind {
    /// Emit once the configuration condition is satisfied.
    Configuration,
    /// Emit when any configured sink receives the tracked object.
    AnySink(Vec<FlowSinkMatcher>),
}

/// The point at which a configured object produces evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub(crate) fn normalize(&mut self) {
        if let FlowCompletionKind::AnySink(sinks) = &mut self.kind {
            for sink in sinks.iter_mut() {
                match &mut sink.kind {
                    FlowSinkMatcherKind::ArgumentOf { call, .. }
                    | FlowSinkMatcherKind::AnyArgumentOf { call } => call.normalize(),
                }
            }
        }
    }
}

// ── FlowSinkMatcher ──────────────────────────────────────────────────────

/// Internal kind of a tracked-object sink matcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlowSinkMatcherKind {
    /// Match one specific argument position.
    ArgumentOf {
        call: MemberCallMatcher,
        index: usize,
    },
    /// Match any argument position at the sink call.
    AnyArgumentOf {
        /// Sink member-call identity.
        call: MemberCallMatcher,
    },
}

/// A tracked object appearing in a selected argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSinkMatcher {
    pub(crate) kind: FlowSinkMatcherKind,
}

impl FlowSinkMatcher {
    /// Borrow the sink matcher kind.
    pub(crate) fn kind(&self) -> &FlowSinkMatcherKind {
        &self.kind
    }

    #[must_use]
    pub fn argument_of(call: MemberCallMatcher, index: usize) -> Self {
        Self {
            kind: FlowSinkMatcherKind::ArgumentOf { call, index },
        }
    }

    #[must_use]
    pub fn any_argument_of(call: MemberCallMatcher) -> Self {
        Self {
            kind: FlowSinkMatcherKind::AnyArgumentOf { call },
        }
    }
}

/// Declarative object lifecycle matching: source, configuration, completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFlowMatcher {
    /// Evidence symbol for the completed flow.
    symbol: String,
    /// Calls that produce tracked objects.
    sources: Vec<ObjectSourceMatcher>,
    /// Configuration condition.
    condition: Option<FlowCondition>,
    /// Completion/emission mode.
    completion: Option<FlowCompletion>,
    /// Deferred builder error, reported when the containing catalog validates.
    invalid_operation: Option<&'static str>,
}

impl ObjectFlowMatcher {
    /// Start an unvalidated builder for a named object flow.
    pub fn builder(symbol: impl Into<String>) -> ObjectFlowMatcherBuilder {
        ObjectFlowMatcherBuilder {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
            invalid_operation: None,
        }
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

    pub(crate) fn invalid_operation(&self) -> Option<&'static str> {
        self.invalid_operation
    }

    pub(crate) fn normalize(&mut self) {
        self.symbol = self.symbol.trim().to_string();
        for source in &mut self.sources {
            source.call.normalize();
        }
        if let Some(condition) = &mut self.condition {
            condition.normalize();
        }
        if let Some(completion) = &mut self.completion {
            completion.normalize();
        }
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
        // Keep the first invalid operation so the builder reports a stable,
        // actionable error instead of silently choosing one configuration.
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

    /// Build an object-flow matcher. Shape validation is deferred until its
    /// containing catalog is built.
    pub fn build(self) -> ObjectFlowMatcher {
        ObjectFlowMatcher {
            symbol: self.symbol,
            sources: self.sources,
            condition: self.condition,
            completion: self.completion,
            invalid_operation: self.invalid_operation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ObjectSourceMatcher {
        ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted("document.createElement"))
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
            .build();
        assert_eq!(matcher.symbol(), "input");
    }

    #[test]
    fn empty_alternatives_and_duplicate_operations_fail() {
        let empty = ObjectFlowMatcher::builder("empty")
            .source(source())
            .configured_by(FlowCondition::any_of(Vec::<ObjectEventMatcher>::new()))
            .complete_at(FlowCompletion::configuration())
            .build();
        assert_eq!(empty.symbol(), "empty");

        let duplicate = ObjectFlowMatcher::builder("duplicate")
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
            .build();
        assert_eq!(duplicate.symbol(), "duplicate");
    }
}
