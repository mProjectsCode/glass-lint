use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::rule::{
    ArgumentConstraint, ValueMatcher,
    query::{
        LifecycleQuery,
        lifecycle::{
            LifecycleCompletionKind, LifecycleConditionKind, LifecycleEvent, LifecycleEventKind,
            LifecycleSink, LifecycleSinkKind,
        },
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    /// Build a compiled flow from a [`LifecycleQuery`] and evidence symbol.
    pub fn from_lifecycle_query(lc: &LifecycleQuery, symbol: &str) -> Self {
        let (requirements, all_requirements_required) = lc.condition.as_ref().map_or_else(
            || (Vec::new(), false),
            |cond| match cond.kind() {
                LifecycleConditionKind::AnyOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    false,
                ),
                LifecycleConditionKind::AllOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    true,
                ),
            },
        );
        let (sinks, emit_on_requirements) = lc.completion.as_ref().map_or_else(
            || (Vec::new(), false),
            |comp| match comp.kind() {
                LifecycleCompletionKind::Configuration => (Vec::new(), true),
                LifecycleCompletionKind::AnySink(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    false,
                ),
            },
        );
        Self {
            symbol: symbol.to_owned(),
            sources: lc
                .sources
                .iter()
                .map(CompiledObjectSource::from_event_query)
                .collect(),
            requirements,
            sinks,
            all_requirements_required,
            emit_on_requirements,
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
    fn from_event_query(eq: &crate::api::rule::query::EventQuery) -> Self {
        let member_call = match &eq.event {
            crate::api::rule::query::EventSpec::MemberCall { member } => member.clone(),
            _ => SymbolPath::default(),
        };
        Self {
            member_call,
            arguments: eq.constraints.clone(),
            is_rooted: true,
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
