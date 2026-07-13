use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowValueMatcher {
    Any,
    StaticExact(Vec<String>),
    StaticPrefix(Vec<String>),
    StaticContainsAny(Vec<String>),
    StaticContainsAll(Vec<String>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCallArgMatcher {
    pub index: usize,
    pub value: FlowValueMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSource {
    pub member_call: String,
    pub arg_strings: Vec<ArgStringMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowRequirement {
    PropertyWrite {
        property: String,
        value: FlowValueMatcher,
    },
    MemberCall {
        member: String,
        args: Vec<FlowCallArgMatcher>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowSinkArgs {
    Any,
    Indices(Vec<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSink {
    pub member_calls: Vec<String>,
    pub args: FlowSinkArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowMatcher {
    pub symbol: String,
    pub sources: Vec<FlowSource>,
    pub requirements: Vec<FlowRequirement>,
    pub sinks: Vec<FlowSink>,
    pub all_requirements_required: bool,
    pub emit_on_requirements: bool,
}

impl FlowMatcher {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            sources: Vec::new(),
            requirements: Vec::new(),
            sinks: Vec::new(),
            all_requirements_required: false,
            emit_on_requirements: false,
        }
    }

    pub fn source_member_call(mut self, member_call: impl Into<String>) -> Self {
        self.sources.push(FlowSource {
            member_call: member_call.into(),
            arg_strings: Vec::new(),
        });
        self
    }

    pub fn source_arg_string<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let Some(source) = self.sources.last_mut() else {
            return self;
        };
        source.arg_strings.push(ArgStringMatcher {
            index,
            values: values.into_iter().map(Into::into).collect(),
            predicate: None,
        });
        self
    }

    pub fn property_write(mut self, property: impl Into<String>, value: FlowValueMatcher) -> Self {
        self.requirements.push(FlowRequirement::PropertyWrite {
            property: property.into(),
            value,
        });
        self
    }

    pub fn member_call_config<I>(mut self, member: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = (usize, FlowValueMatcher)>,
    {
        self.requirements.push(FlowRequirement::MemberCall {
            member: member.into(),
            args: args
                .into_iter()
                .map(|(index, value)| FlowCallArgMatcher { index, value })
                .collect(),
        });
        self
    }

    pub fn require_all(mut self) -> Self {
        self.all_requirements_required = true;
        self
    }

    pub fn emit_when_requirements_met(mut self) -> Self {
        self.emit_on_requirements = true;
        self
    }

    pub fn sink_member_call_arg_indices<I, S, J>(mut self, member_calls: I, indices: J) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        J: IntoIterator<Item = usize>,
    {
        self.sinks.push(FlowSink {
            member_calls: member_calls.into_iter().map(Into::into).collect(),
            args: FlowSinkArgs::Indices(indices.into_iter().collect()),
        });
        self
    }

    pub fn sink_member_call_any_arg<I, S>(mut self, member_calls: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.sinks.push(FlowSink {
            member_calls: member_calls.into_iter().map(Into::into).collect(),
            args: FlowSinkArgs::Any,
        });
        self
    }

    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }
}
