use smol_str::SmolStr;

use crate::{
    analysis::SymbolPath,
    api::rule::{
        ArgumentConstraint, FlowSinkMatcher, ObjectEventMatcher, ObjectFlowMatcher,
        ObjectSourceMatcher, ValueMatcher,
        matcher::{
            FlowCompletionKind, FlowConditionKind, FlowSinkMatcherKind, ObjectEventMatcherKind,
        },
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledObjectFlow {
    pub(crate) symbol: String,
    pub(crate) sources: Vec<CompiledObjectSource>,
    pub(crate) requirements: Vec<CompiledObjectRequirement>,
    pub(crate) sinks: Vec<CompiledObjectSink>,
    pub(crate) all_requirements_required: bool,
    pub(crate) emit_on_requirements: bool,
}

impl CompiledObjectFlow {
    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }

    pub fn requirements_ready(&self, completed: usize) -> bool {
        if self.all_requirements_required {
            completed == self.requirements.len()
        } else {
            completed != 0
        }
    }

    pub fn from_matcher(flow: &ObjectFlowMatcher) -> Self {
        let (requirements, all_requirements_required) = flow.condition().map_or_else(
            || (Vec::new(), false),
            |cond| match cond.kind() {
                FlowConditionKind::AnyOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    false,
                ),
                FlowConditionKind::AllOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    true,
                ),
            },
        );
        let (sinks, emit_on_requirements) = flow.completion().map_or_else(
            || (Vec::new(), false),
            |comp| match comp.kind() {
                FlowCompletionKind::Configuration => (Vec::new(), true),
                FlowCompletionKind::AnySink(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    false,
                ),
            },
        );
        Self {
            symbol: flow.symbol().to_owned(),
            sources: flow
                .sources()
                .iter()
                .map(CompiledObjectSource::from_matcher)
                .collect(),
            requirements,
            sinks,
            all_requirements_required,
            emit_on_requirements,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledObjectSource {
    pub(crate) member_call: SymbolPath,
    pub(crate) arguments: Vec<ArgumentConstraint>,
    pub(crate) is_rooted: bool,
}

impl CompiledObjectSource {
    fn from_matcher(source: &ObjectSourceMatcher) -> Self {
        Self {
            member_call: SymbolPath::from(source.chain()),
            arguments: source.arguments().to_vec(),
            is_rooted: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompiledObjectRequirement {
    PropertyWrite {
        property: SmolStr,
        value: ValueMatcher,
    },
    MemberCall {
        member: SymbolPath,
        arguments: Vec<ArgumentConstraint>,
    },
}

impl CompiledObjectRequirement {
    fn from_matcher(event: &ObjectEventMatcher) -> Self {
        match event.kind() {
            ObjectEventMatcherKind::PropertyWrite { property, value } => Self::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            },
            ObjectEventMatcherKind::MemberCall { member, arguments } => Self::MemberCall {
                member: SymbolPath::from(member.as_str()),
                arguments: arguments.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompiledObjectSinkArguments {
    Any,
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArguments {
    pub fn present_indices(&self, argument_count: usize) -> Vec<usize> {
        match self {
            Self::Any => (0..argument_count).collect(),
            Self::Indices(indices) => indices
                .iter()
                .copied()
                .filter(|index| *index < argument_count)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledObjectSink {
    pub(crate) member_calls: Vec<SymbolPath>,
    pub(crate) args: CompiledObjectSinkArguments,
    pub(crate) is_rooted: bool,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &FlowSinkMatcher) -> Self {
        match sink.kind() {
            FlowSinkMatcherKind::ArgumentOf { chain, index } => Self {
                member_calls: vec![SymbolPath::from(chain.as_str())],
                args: CompiledObjectSinkArguments::Indices(vec![*index]),
                is_rooted: true,
            },
            FlowSinkMatcherKind::AnyArgumentOf { chain } => Self {
                member_calls: vec![SymbolPath::from(chain.as_str())],
                args: CompiledObjectSinkArguments::Any,
                is_rooted: true,
            },
        }
    }
}
