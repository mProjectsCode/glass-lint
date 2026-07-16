use super::MemberCallMatcher;

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueMatcher {
    pub kind: ValueMatcherKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueMatcherKind {
    Any,
    StaticString(StaticStringPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticStringPredicate {
    Any,
    Exact(Vec<String>),
    Prefix(Vec<String>),
    ContainsAny(Vec<String>),
    ContainsAll(Vec<String>),
}

impl ValueMatcher {
    fn with_static_predicate(mut self, predicate: StaticStringPredicate) -> Self {
        self.kind = ValueMatcherKind::StaticString(predicate);
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
        Self {
            kind: ValueMatcherKind::StaticString(StaticStringPredicate::Any),
        }
    }

    #[must_use]
    pub fn equals(self, value: impl Into<String>) -> Self {
        self.with_static_predicate(StaticStringPredicate::Exact(vec![value.into()]))
    }

    #[must_use]
    pub fn equals_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicate::Exact(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn starts_with_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicate::Prefix(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn contains_any<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicate::ContainsAny(
            values.into_iter().map(Into::into).collect(),
        ))
    }

    #[must_use]
    pub fn contains_all<I, S>(self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_static_predicate(StaticStringPredicate::ContainsAll(
            values.into_iter().map(Into::into).collect(),
        ))
    }
}

/// A predicate applied to one selected call argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentMatcher {
    Value(ValueMatcher),
    ObjectKeys(Vec<String>),
    RootedExpressions(Vec<String>),
}

impl ArgumentMatcher {
    pub fn object_keys<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::ObjectKeys(keys.into_iter().map(Into::into).collect())
    }

    pub fn rooted_expressions<I, S>(chains: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::RootedExpressions(chains.into_iter().map(Into::into).collect())
    }
}

impl From<ValueMatcher> for ArgumentMatcher {
    fn from(value: ValueMatcher) -> Self {
        Self::Value(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentConstraint {
    pub index: usize,
    pub matcher: ArgumentMatcher,
}

/// A call that returns the object tracked by an [`ObjectFlowMatcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSourceMatcher {
    pub call: MemberCallMatcher,
}

impl ObjectSourceMatcher {
    #[must_use]
    pub fn returned_by(call: MemberCallMatcher) -> Self {
        Self { call }
    }
}

/// A lifecycle event observed on a tracked object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectEventMatcher {
    PropertyWrite {
        property: String,
        value: ValueMatcher,
    },
    MemberCall {
        member: String,
        arguments: Vec<ArgumentConstraint>,
    },
}

impl ObjectEventMatcher {
    pub fn property_write(property: impl Into<String>, value: ValueMatcher) -> Self {
        Self::PropertyWrite {
            property: property.into(),
            value,
        }
    }

    pub fn member_call(member: impl Into<String>) -> ObjectEventBuilder {
        ObjectEventBuilder {
            event: Self::MemberCall {
                member: member.into(),
                arguments: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEventBuilder {
    event: ObjectEventMatcher,
}

impl ObjectEventBuilder {
    #[must_use]
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        if let ObjectEventMatcher::MemberCall { arguments, .. } = &mut self.event {
            arguments.push(ArgumentConstraint {
                index,
                matcher: matcher.into(),
            });
        }
        self
    }

    pub fn build(self) -> ObjectEventMatcher {
        self.event
    }
}

impl From<ObjectEventBuilder> for ObjectEventMatcher {
    fn from(value: ObjectEventBuilder) -> Self {
        value.build()
    }
}

/// Explicitly combines the events that configure a tracked object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowCondition {
    AnyOf(Vec<ObjectEventMatcher>),
    AllOf(Vec<ObjectEventMatcher>),
}

impl FlowCondition {
    pub fn any_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<ObjectEventMatcher>,
    {
        Self::AnyOf(events.into_iter().map(Into::into).collect())
    }

    pub fn all_of<I>(events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<ObjectEventMatcher>,
    {
        Self::AllOf(events.into_iter().map(Into::into).collect())
    }

    pub fn event(event: impl Into<ObjectEventMatcher>) -> Self {
        Self::AllOf(vec![event.into()])
    }
}

/// The point at which a configured object produces evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowCompletion {
    Configuration,
    AnySink(Vec<FlowSinkMatcher>),
}

impl FlowCompletion {
    #[must_use]
    pub fn configuration() -> Self {
        Self::Configuration
    }

    pub fn any_sink<I>(sinks: I) -> Self
    where
        I: IntoIterator<Item = FlowSinkMatcher>,
    {
        Self::AnySink(sinks.into_iter().collect())
    }
}

/// A tracked object appearing in a selected argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowSinkMatcher {
    ArgumentOf {
        call: MemberCallMatcher,
        index: usize,
    },
    AnyArgumentOf {
        call: MemberCallMatcher,
    },
}

impl FlowSinkMatcher {
    #[must_use]
    pub fn argument_of(call: MemberCallMatcher, index: usize) -> Self {
        Self::ArgumentOf { call, index }
    }

    #[must_use]
    pub fn any_argument_of(call: MemberCallMatcher) -> Self {
        Self::AnyArgumentOf { call }
    }
}

/// Declarative object lifecycle matching: source, configuration, completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFlowMatcher {
    pub symbol: String,
    pub sources: Vec<ObjectSourceMatcher>,
    pub condition: Option<FlowCondition>,
    pub completion: Option<FlowCompletion>,
}

impl ObjectFlowMatcher {
    pub fn builder(symbol: impl Into<String>) -> ObjectFlowMatcherBuilder {
        ObjectFlowMatcherBuilder {
            symbol: symbol.into(),
            sources: Vec::new(),
            condition: None,
            completion: None,
            invalid_operation: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectFlowMatcherBuilder {
    symbol: String,
    sources: Vec<ObjectSourceMatcher>,
    condition: Option<FlowCondition>,
    completion: Option<FlowCompletion>,
    invalid_operation: Option<&'static str>,
}

impl ObjectFlowMatcherBuilder {
    #[must_use]
    pub fn source(mut self, source: ObjectSourceMatcher) -> Self {
        self.sources.push(source);
        self
    }

    #[must_use]
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
    pub fn complete_at(mut self, completion: FlowCompletion) -> Self {
        if self.completion.is_some() {
            self.invalid_operation = Some("complete_at may only be specified once");
        } else {
            self.completion = Some(completion);
        }
        self
    }

    pub fn build(self) -> Result<ObjectFlowMatcher, String> {
        if let Some(error) = self.invalid_operation {
            return Err(error.into());
        }
        let matcher = ObjectFlowMatcher {
            symbol: self.symbol,
            sources: self.sources,
            condition: self.condition,
            completion: self.completion,
        };
        super::super::validation::validate_object_flow(&matcher, "flow")?;
        Ok(matcher)
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
        assert!(matcher.is_ok());
    }

    #[test]
    fn empty_alternatives_and_duplicate_operations_fail() {
        let empty = ObjectFlowMatcher::builder("empty")
            .source(source())
            .configured_by(FlowCondition::any_of(Vec::<ObjectEventMatcher>::new()))
            .complete_at(FlowCompletion::configuration())
            .build();
        assert!(empty.unwrap_err().contains("alternatives"));

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
        assert!(duplicate.unwrap_err().contains("configured_by"));
    }
}
