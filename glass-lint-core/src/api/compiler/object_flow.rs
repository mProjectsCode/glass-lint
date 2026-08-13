use std::{ops::Range, slice::Iter};

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::{
    analysis::model::flow::{FlowReadiness, RequirementReadiness, SinkReadiness},
    api::{
        compiler::{
            normalized::{
                CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle,
                NormalizedLifecycleCompletion, NormalizedLifecycleCondition,
                NormalizedLifecycleEvent, NormalizedLifecycleSink,
            },
            validate::{LifecycleSource, SubjectRelationError, classify_lifecycle_source},
        },
        rule::{ValueMatcher, query::lifecycle::LifecycleCallTarget},
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
    symbol: SmolStr,
    sources: Vec<CompiledObjectSource>,
    requirements: Vec<CompiledObjectRequirement>,
    sinks: Vec<CompiledObjectSink>,
    requirement_mode: RequirementMode,
    completion_mode: CompletionMode,
}

impl CompiledObjectFlow {
    pub(crate) fn readiness(&self) -> FlowReadiness {
        FlowReadiness::new(
            match self.requirement_mode {
                RequirementMode::AllRequired => RequirementReadiness::All,
                RequirementMode::AnyRequired => RequirementReadiness::Any,
            },
            self.requirement_count(),
            match self.completion_mode {
                CompletionMode::Configuration => SinkReadiness::Configuration,
                CompletionMode::AnySink => SinkReadiness::Any,
                CompletionMode::AllSinks => SinkReadiness::All,
            },
            self.sink_count(),
        )
    }

    pub fn evidence_symbol(&self) -> &SmolStr {
        &self.symbol
    }

    pub(crate) fn sources(&self) -> impl Iterator<Item = &CompiledObjectSource> {
        self.sources.iter()
    }

    pub(crate) fn requirements(&self) -> impl Iterator<Item = &CompiledObjectRequirement> {
        self.requirements.iter()
    }

    pub(crate) fn sinks(&self) -> impl Iterator<Item = &CompiledObjectSink> {
        self.sinks.iter()
    }

    pub(crate) fn requirement_count(&self) -> usize {
        self.requirements.len()
    }

    pub(crate) fn has_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    pub(crate) fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    pub(crate) fn completion_mode(&self) -> CompletionMode {
        self.completion_mode
    }

    #[cfg(test)]
    pub(crate) fn test_with_evidence_counts(requirements: usize, sinks: usize) -> Self {
        Self {
            symbol: "test".into(),
            sources: vec![CompiledObjectSource {
                target: LifecycleCallTarget::Global("source".into()),
                arguments: CanonicalArgumentConstraints::default(),
            }],
            requirements: (0..requirements)
                .map(|_| CompiledObjectRequirement::PropertyWrite {
                    property: "property".into(),
                    value: ValueMatcher::any_value(),
                })
                .collect(),
            sinks: (0..sinks)
                .map(|_| CompiledObjectSink {
                    target: LifecycleCallTarget::Global("sink".into()),
                    args: CompiledObjectSinkArguments::Any,
                })
                .collect(),
            requirement_mode: RequirementMode::AllRequired,
            completion_mode: CompletionMode::AllSinks,
        }
    }

    /// Build a compiled flow directly from the normalized lifecycle IR.
    pub(crate) fn from_normalized_lifecycle(
        lc: &NormalizedLifecycle,
        symbol: &str,
    ) -> Result<Self, SubjectRelationError> {
        let (requirements, requirement_mode) = lc.condition().map_or_else(
            || (Vec::new(), RequirementMode::AnyRequired),
            |cond| match cond {
                NormalizedLifecycleCondition::AnyOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_matcher)
                        .collect(),
                    RequirementMode::AnyRequired,
                ),
                NormalizedLifecycleCondition::AllOf(events) => (
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
            |comp| match comp {
                NormalizedLifecycleCompletion::Configuration => {
                    (Vec::new(), CompletionMode::Configuration)
                }
                NormalizedLifecycleCompletion::AnySink(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    CompletionMode::AnySink,
                ),
                NormalizedLifecycleCompletion::AllSinks(sinks) => (
                    sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                    CompletionMode::AllSinks,
                ),
            },
        );
        let sources = lc
            .sources()
            .iter()
            .map(CompiledObjectSource::from_normalized_event)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            symbol: SmolStr::new(symbol),
            sources,
            requirements,
            sinks,
            requirement_mode,
            completion_mode,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectSource {
    target: LifecycleCallTarget,
    arguments: CanonicalArgumentConstraints,
}

impl CompiledObjectSource {
    fn from_normalized_event(event: &NormalizedEvent) -> Result<Self, SubjectRelationError> {
        let target = match classify_lifecycle_source(event.identity(), event.event())? {
            LifecycleSource::GlobalCall { name } => LifecycleCallTarget::Global(name.clone()),
            LifecycleSource::RootedMember { member } => {
                LifecycleCallTarget::RootedMember(member.clone())
            }
        };
        Ok(Self {
            target,
            arguments: event.arguments().clone(),
        })
    }

    pub(crate) fn target(&self) -> &LifecycleCallTarget {
        &self.target
    }

    #[cfg(test)]
    pub(crate) fn arguments(&self) -> &CanonicalArgumentConstraints {
        &self.arguments
    }

    pub(crate) fn argument_constraints(&self) -> &CanonicalArgumentConstraints {
        &self.arguments
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
        arguments: CanonicalArgumentConstraints,
    },
}

impl CompiledObjectRequirement {
    fn from_matcher(event: &NormalizedLifecycleEvent) -> Self {
        match event {
            NormalizedLifecycleEvent::PropertyWrite { property, value } => Self::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            },
            NormalizedLifecycleEvent::MemberCall { member, arguments } => Self::MemberCall {
                member: SymbolPath::from(member.as_str()),
                arguments: arguments.clone(),
            },
        }
    }

    pub(crate) fn member_call(&self) -> Option<(&SymbolPath, &CanonicalArgumentConstraints)> {
        match self {
            Self::MemberCall { member, arguments } => Some((member, arguments)),
            Self::PropertyWrite { .. } => None,
        }
    }

    pub(crate) fn property_write(&self) -> Option<(&SmolStr, &ValueMatcher)> {
        match self {
            Self::PropertyWrite { property, value } => Some((property, value)),
            Self::MemberCall { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CompiledObjectSinkArguments {
    Any,
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArguments {
    pub(crate) fn present_indices(&self, argument_count: usize) -> PresentIndices<'_> {
        match self {
            Self::Any => PresentIndices::Any(0..argument_count),
            Self::Indices(indices) => PresentIndices::Indices {
                iter: indices.iter(),
                argument_count,
            },
        }
    }
}

pub(crate) enum PresentIndices<'a> {
    Any(Range<usize>),
    Indices {
        iter: Iter<'a, usize>,
        argument_count: usize,
    },
}

impl Iterator for PresentIndices<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Any(range) => range.next(),
            Self::Indices {
                iter,
                argument_count,
            } => iter.find(|index| **index < *argument_count).copied(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectSink {
    target: LifecycleCallTarget,
    args: CompiledObjectSinkArguments,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &NormalizedLifecycleSink) -> Self {
        match sink {
            NormalizedLifecycleSink::ArgumentOf { target, index } => Self {
                target: target.clone(),
                args: CompiledObjectSinkArguments::Indices(vec![*index]),
            },
            NormalizedLifecycleSink::AnyArgumentOf { target } => Self {
                target: target.clone(),
                args: CompiledObjectSinkArguments::Any,
            },
        }
    }

    pub(crate) fn target(&self) -> &LifecycleCallTarget {
        &self.target
    }

    pub(crate) fn arguments(&self) -> &CompiledObjectSinkArguments {
        &self.args
    }

    #[cfg(test)]
    pub(crate) fn fixed_argument(&self) -> Option<usize> {
        match &self.args {
            CompiledObjectSinkArguments::Any => None,
            CompiledObjectSinkArguments::Indices(indices) => indices.first().copied(),
        }
    }
}
