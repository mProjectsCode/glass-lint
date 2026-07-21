use smol_str::SmolStr;

use crate::{
    analysis::SymbolPath,
    api::rule::{
        ArgumentConstraint, FlowCompletion, FlowCondition, FlowSinkMatcher, MemberCallProvenance,
        ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ValueMatcher,
    },
};

#[derive(Debug, Clone)]
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

    pub fn sink_matches(&self, chain: Option<&SymbolPath>, rooted: bool, argument: usize) -> bool {
        self.sinks.iter().any(|sink| {
            sink.member_calls.iter().any(|member| chain == Some(member))
                && sink.provenance.matches_rooted(rooted)
                && match &sink.args {
                    CompiledObjectSinkArguments::Any => true,
                    CompiledObjectSinkArguments::Indices(indices) => indices.contains(&argument),
                }
        })
    }

    pub fn requirements_ready(&self, completed: usize) -> bool {
        if self.all_requirements_required {
            completed == self.requirements.len()
        } else {
            completed != 0
        }
    }

    pub fn from_matcher(flow: &ObjectFlowMatcher) -> Self {
        let (requirements, all_requirements_required) = match flow.condition.as_ref() {
            Some(FlowCondition::AnyOf(events)) => (
                events
                    .iter()
                    .map(CompiledObjectRequirement::from_matcher)
                    .collect(),
                false,
            ),
            Some(FlowCondition::AllOf(events)) => (
                events
                    .iter()
                    .map(CompiledObjectRequirement::from_matcher)
                    .collect(),
                true,
            ),
            None => (Vec::new(), false),
        };
        let (sinks, emit_on_requirements) = match flow.completion.as_ref() {
            Some(FlowCompletion::Configuration) => (Vec::new(), true),
            Some(FlowCompletion::AnySink(sinks)) => (
                sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                false,
            ),
            None => (Vec::new(), false),
        };
        Self {
            symbol: flow.symbol.clone(),
            sources: flow
                .sources
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

#[derive(Debug, Clone)]
pub(crate) struct CompiledObjectSource {
    pub(crate) member_call: SymbolPath,
    pub(crate) arguments: Vec<ArgumentConstraint>,
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSource {
    fn from_matcher(source: &ObjectSourceMatcher) -> Self {
        Self {
            member_call: SymbolPath::from(source.call.chain()),
            arguments: source.call.arguments().to_vec(),
            provenance: source.call.provenance.clone(),
        }
    }
}

#[derive(Debug, Clone)]
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
        match event {
            ObjectEventMatcher::PropertyWrite { property, value } => Self::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            },
            ObjectEventMatcher::MemberCall { member, arguments } => Self::MemberCall {
                member: SymbolPath::from(member.as_str()),
                arguments: arguments.clone(),
            },
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub(crate) struct CompiledObjectSink {
    pub(crate) member_calls: Vec<SymbolPath>,
    pub(crate) args: CompiledObjectSinkArguments,
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &FlowSinkMatcher) -> Self {
        match sink {
            FlowSinkMatcher::ArgumentOf { call, index } => Self {
                member_calls: vec![SymbolPath::from(call.chain())],
                args: CompiledObjectSinkArguments::Indices(vec![*index]),
                provenance: call.provenance.clone(),
            },
            FlowSinkMatcher::AnyArgumentOf { call } => Self {
                member_calls: vec![SymbolPath::from(call.chain())],
                args: CompiledObjectSinkArguments::Any,
                provenance: call.provenance.clone(),
            },
        }
    }
}
