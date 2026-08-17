use std::ops::Range;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectFlow {
    symbol: SmolStr,
    sources: Vec<CompiledObjectSource>,
    requirements: Vec<CompiledObjectRequirement>,
    sinks: Vec<CompiledObjectSink>,
    requirement_readiness: RequirementReadiness,
    sink_readiness: SinkReadiness,
}

impl CompiledObjectFlow {
    pub(crate) fn readiness(&self) -> FlowReadiness {
        FlowReadiness::new(
            self.requirement_readiness,
            self.requirement_count(),
            self.sink_readiness,
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

    pub(crate) fn sink_readiness(&self) -> SinkReadiness {
        self.sink_readiness
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
            requirement_readiness: RequirementReadiness::All,
            sink_readiness: SinkReadiness::All,
        }
    }

    /// Build a compiled flow directly from the normalized lifecycle IR.
    pub(crate) fn from_normalized_lifecycle(
        lc: &NormalizedLifecycle,
        symbol: &str,
    ) -> Result<Self, SubjectRelationError> {
        let (requirements, requirement_readiness) = lc.condition().map_or_else(
            || (Vec::new(), RequirementReadiness::Any),
            |cond| match cond {
                NormalizedLifecycleCondition::AnyOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_normalized_lifecycle_event)
                        .collect(),
                    RequirementReadiness::Any,
                ),
                NormalizedLifecycleCondition::AllOf(events) => (
                    events
                        .iter()
                        .map(CompiledObjectRequirement::from_normalized_lifecycle_event)
                        .collect(),
                    RequirementReadiness::All,
                ),
            },
        );
        let (sinks, sink_readiness) = match lc.completion() {
            NormalizedLifecycleCompletion::Configuration => {
                (Vec::new(), SinkReadiness::Configuration)
            }
            NormalizedLifecycleCompletion::AnySink(sinks) => (
                sinks
                    .iter()
                    .map(CompiledObjectSink::from_normalized_lifecycle_sink)
                    .collect(),
                SinkReadiness::Any,
            ),
            NormalizedLifecycleCompletion::AllSinks(sinks) => (
                sinks
                    .iter()
                    .map(CompiledObjectSink::from_normalized_lifecycle_sink)
                    .collect(),
                SinkReadiness::All,
            ),
        };
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
            requirement_readiness,
            sink_readiness,
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
    fn from_normalized_lifecycle_event(event: &NormalizedLifecycleEvent) -> Self {
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
    Single(usize),
}

impl CompiledObjectSinkArguments {
    pub(crate) fn matches_argument(&self, argument: usize) -> bool {
        match self {
            Self::Any => true,
            Self::Single(index) => *index == argument,
        }
    }

    pub(crate) fn present_indices(&self, argument_count: usize) -> PresentIndices {
        match self {
            Self::Any => PresentIndices::Any(0..argument_count),
            Self::Single(index) => PresentIndices::Single {
                index: *index,
                argument_count,
                yielded: false,
            },
        }
    }
}

pub(crate) enum PresentIndices {
    Any(Range<usize>),
    Single {
        index: usize,
        argument_count: usize,
        yielded: bool,
    },
}

impl Iterator for PresentIndices {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Any(range) => range.next(),
            Self::Single {
                index,
                argument_count,
                yielded,
            } => {
                if *yielded {
                    None
                } else {
                    *yielded = true;
                    (*index < *argument_count).then_some(*index)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CompiledObjectSink {
    target: LifecycleCallTarget,
    args: CompiledObjectSinkArguments,
}

impl CompiledObjectSink {
    fn from_normalized_lifecycle_sink(sink: &NormalizedLifecycleSink) -> Self {
        match sink {
            NormalizedLifecycleSink::ArgumentOf { target, index } => Self {
                target: target.clone(),
                args: CompiledObjectSinkArguments::Single(*index),
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
            CompiledObjectSinkArguments::Single(index) => Some(*index),
        }
    }
}
