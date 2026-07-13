use super::MemberCallMatcher;

/// A context-independent predicate over an argument value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueMatcher {
    pub(crate) kind: ValueMatcherKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValueMatcherKind {
    Any,
    StaticString(StaticStringPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaticStringPredicate {
    Any,
    Exact(Vec<String>),
    Prefix(Vec<String>),
    ContainsAny(Vec<String>),
    ContainsAll(Vec<String>),
}

impl ValueMatcher {
    /// Matches both proven static values and dynamic or unknown values.
    pub fn any_value() -> Self {
        Self {
            kind: ValueMatcherKind::Any,
        }
    }

    /// Starts a predicate that requires a proven static string.
    pub fn static_string() -> Self {
        Self {
            kind: ValueMatcherKind::StaticString(StaticStringPredicate::Any),
        }
    }

    pub fn equals(mut self, value: impl Into<String>) -> Self {
        self.kind =
            ValueMatcherKind::StaticString(StaticStringPredicate::Exact(vec![value.into()]));
        self
    }

    pub fn equals_any<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::Exact(
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn starts_with_any<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::Prefix(
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn contains_any<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::ContainsAny(
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn contains_all<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.kind = ValueMatcherKind::StaticString(StaticStringPredicate::ContainsAll(
            values.into_iter().map(Into::into).collect(),
        ));
        self
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
    pub(crate) index: usize,
    pub(crate) matcher: ArgumentMatcher,
}

/// A call that returns the object tracked by an [`ObjectFlowMatcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSourceMatcher {
    pub(crate) call: MemberCallMatcher,
}

impl ObjectSourceMatcher {
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
    pub fn argument_of(call: MemberCallMatcher, index: usize) -> Self {
        Self::ArgumentOf { call, index }
    }

    pub fn any_argument_of(call: MemberCallMatcher) -> Self {
        Self::AnyArgumentOf { call }
    }
}

/// Declarative object lifecycle matching: source, configuration, completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFlowMatcher {
    pub(crate) symbol: String,
    pub(crate) sources: Vec<ObjectSourceMatcher>,
    pub(crate) condition: Option<FlowCondition>,
    pub(crate) completion: Option<FlowCompletion>,
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
    pub fn source(mut self, source: ObjectSourceMatcher) -> Self {
        self.sources.push(source);
        self
    }

    pub fn configured_by(mut self, condition: FlowCondition) -> Self {
        if self.condition.is_some() {
            self.invalid_operation = Some("configured_by may only be specified once");
        } else {
            self.condition = Some(condition);
        }
        self
    }

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

/// Legacy construction shim retained only for the in-tree migration. New
/// rules should use `ObjectFlowMatcher::builder` and explicit conditions.
#[derive(Debug, Clone)]
pub struct FlowMatcher {
    symbol: String,
    sources: Vec<ObjectSourceMatcher>,
    events: Vec<ObjectEventMatcher>,
    all: bool,
    completion: Option<FlowCompletion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowValueMatcher {
    Any,
    StaticExact(Vec<String>),
    StaticPrefix(Vec<String>),
    StaticContainsAny(Vec<String>),
    StaticContainsAll(Vec<String>),
}

impl From<FlowValueMatcher> for ValueMatcher {
    fn from(value: FlowValueMatcher) -> Self {
        match value {
            FlowValueMatcher::Any => ValueMatcher::any_value(),
            FlowValueMatcher::StaticExact(values) => {
                ValueMatcher::static_string().equals_any(values)
            }
            FlowValueMatcher::StaticPrefix(values) => {
                ValueMatcher::static_string().starts_with_any(values)
            }
            FlowValueMatcher::StaticContainsAny(values) => {
                ValueMatcher::static_string().contains_any(values)
            }
            FlowValueMatcher::StaticContainsAll(values) => {
                ValueMatcher::static_string().contains_all(values)
            }
        }
    }
}

impl FlowMatcher {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sources: Vec::new(),
            events: Vec::new(),
            all: false,
            completion: None,
        }
    }
    pub fn source_member_call(mut self, member_call: impl Into<String>) -> Self {
        self.sources
            .push(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                member_call,
            )));
        self
    }
    pub fn source_arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if let Some(source) = self.sources.last_mut() {
            source.call = source
                .call
                .clone()
                .arg(index, ValueMatcher::static_string().equals_any(values));
        } else {
            self.sources
                .push(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                    "",
                )));
        }
        self
    }
    pub fn property_write(mut self, property: impl Into<String>, value: FlowValueMatcher) -> Self {
        self.events
            .push(ObjectEventMatcher::property_write(property, value.into()));
        self
    }
    pub fn member_call_config<I>(mut self, member: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = (usize, FlowValueMatcher)>,
    {
        let event = args
            .into_iter()
            .fold(
                ObjectEventMatcher::member_call(member),
                |event, (index, value)| event.arg(index, ValueMatcher::from(value)),
            )
            .build();
        self.events.push(event);
        self
    }
    pub fn require_all(mut self) -> Self {
        self.all = true;
        self
    }
    pub fn emit_when_requirements_met(mut self) -> Self {
        self.completion = Some(FlowCompletion::Configuration);
        self
    }
    pub fn sink_member_call_arg_indices<I, S, J>(mut self, member_calls: I, indices: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = usize>,
    {
        let indices = indices.into_iter().collect::<Vec<_>>();
        let sinks: Vec<_> = member_calls
            .into_iter()
            .flat_map(|member| {
                let member = member.into();
                indices.iter().copied().map(move |index| {
                    FlowSinkMatcher::argument_of(MemberCallMatcher::rooted(member.clone()), index)
                })
            })
            .collect();
        self.completion = Some(FlowCompletion::any_sink(sinks));
        self
    }
    pub fn sink_member_call_any_arg<I, S>(mut self, member_calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.completion = Some(FlowCompletion::any_sink(member_calls.into_iter().map(
            |member| FlowSinkMatcher::any_argument_of(MemberCallMatcher::rooted(member)),
        )));
        self
    }
}

impl From<FlowMatcher> for ObjectFlowMatcher {
    fn from(value: FlowMatcher) -> Self {
        ObjectFlowMatcher {
            symbol: value.symbol,
            sources: value.sources,
            condition: (!value.events.is_empty()).then(|| {
                if value.all {
                    FlowCondition::all_of(value.events)
                } else {
                    FlowCondition::any_of(value.events)
                }
            }),
            completion: value.completion,
        }
    }
}

impl From<FlowMatcher> for super::Matcher {
    fn from(value: FlowMatcher) -> Self {
        Self::ObjectFlow(value.into())
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
