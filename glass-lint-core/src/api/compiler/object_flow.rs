use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    compiler::normalized::{NormalizedEvent, NormalizedLifecycle},
    rule::{
        ArgumentConstraint, ValueMatcher,
        query::{
            EventSpec, IdentitySpec,
            lifecycle::{
                LifecycleCompletionKind, LifecycleConditionKind, LifecycleEvent,
                LifecycleEventKind, LifecycleSink, LifecycleSinkKind,
            },
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RequirementMode {
    AllRequired,
    AnyRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CompletionMode {
    Configuration,
    AnySink,
    AllSinks,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectFlow {
    pub(crate) symbol: SmolStr,
    pub(crate) sources: Vec<CompiledObjectSource>,
    pub(crate) requirements: Vec<CompiledObjectRequirement>,
    pub(crate) sinks: Vec<CompiledObjectSink>,
    pub(crate) requirement_mode: RequirementMode,
    pub(crate) completion_mode: CompletionMode,
}

impl CompiledObjectFlow {
    pub fn evidence_symbol(&self) -> &SmolStr {
        &self.symbol
    }

    pub fn requirements_ready(&self, completed: usize) -> bool {
        match self.requirement_mode {
            RequirementMode::AllRequired => completed == self.requirements.len(),
            RequirementMode::AnyRequired => completed != 0,
        }
    }

    /// Build a compiled flow directly from the normalized lifecycle IR.
    pub(crate) fn from_normalized_lifecycle(lc: &NormalizedLifecycle, symbol: &str) -> Self {
        let (requirements, requirement_mode) = lc.condition().map_or_else(
            || (Vec::new(), RequirementMode::AnyRequired),
            |cond| match cond.kind() {
                LifecycleConditionKind::AnyOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    RequirementMode::AnyRequired,
                ),
                LifecycleConditionKind::AllOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    RequirementMode::AllRequired,
                ),
            },
        );
        let (sinks, completion_mode) = lc.completion().map_or_else(
            || (Vec::new(), CompletionMode::AnySink),
            |comp| match comp.kind() {
                LifecycleCompletionKind::Configuration => {
                    (Vec::new(), CompletionMode::Configuration)
                }
                LifecycleCompletionKind::AnySink(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    CompletionMode::AnySink,
                ),
                LifecycleCompletionKind::AllSinks(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    CompletionMode::AllSinks,
                ),
            },
        );
        Self {
            symbol: SmolStr::new(symbol),
            sources: lc
                .sources()
                .iter()
                .map(CompiledObjectSource::from_normalized_event)
                .collect(),
            requirements,
            sinks,
            requirement_mode,
            completion_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectSource {
    pub(crate) member_call: SymbolPath,
    pub(crate) arguments: Vec<ArgumentConstraint>,
    pub(crate) is_rooted: bool,
}

impl CompiledObjectSource {
    fn from_normalized_event(event: &NormalizedEvent) -> Self {
        let member_call = match event.event() {
            EventSpec::MemberCall { member } => member.clone(),
            _ => SymbolPath::default(),
        };
        Self {
            member_call,
            arguments: event.arguments().to_flat_vec(),
            is_rooted: matches!(event.identity(), Some(IdentitySpec::Rooted { .. })),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    fn from_matcher(event: &LifecycleEvent) -> Self {
        match event.kind() {
            LifecycleEventKind::PropertyWrite { property, value } => Self::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            },
            LifecycleEventKind::MemberCall { member, arguments } => Self::MemberCall {
                member: SymbolPath::from(member.as_str()),
                arguments: arguments.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CompiledObjectSinkArguments {
    Any,
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArguments {
    pub fn present_indices<'a>(
        &'a self,
        argument_count: usize,
    ) -> Box<dyn Iterator<Item = usize> + 'a> {
        match self {
            Self::Any => Box::new(0..argument_count),
            Self::Indices(indices) => {
                Box::new(indices.iter().copied().filter(move |i| *i < argument_count))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectSink {
    pub(crate) member_calls: Vec<SymbolPath>,
    pub(crate) args: CompiledObjectSinkArguments,
    pub(crate) is_rooted: bool,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &LifecycleSink) -> Self {
        match sink.kind() {
            LifecycleSinkKind::ArgumentOf { chain, index } => Self {
                member_calls: vec![SymbolPath::from(chain.as_str())],
                args: CompiledObjectSinkArguments::Indices(vec![*index]),
                is_rooted: true,
            },
            LifecycleSinkKind::AnyArgumentOf { chain } => Self {
                member_calls: vec![SymbolPath::from(chain.as_str())],
                args: CompiledObjectSinkArguments::Any,
                is_rooted: true,
            },
        }
    }
}
