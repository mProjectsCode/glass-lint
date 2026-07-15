use super::super::rule::{
    ApiMatcher, ArgumentConstraint, FlowCompletion, FlowCondition, FlowSinkMatcher,
    MemberCallProvenance, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
    ValueMatcher,
};

/// Canonical matcher representation consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    pub(crate) matcher: ApiMatcher,
    pub(crate) flows: Vec<CompiledObjectFlow>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherCatalog<'a> {
    pub(crate) rules: &'a [CompiledRule],
    pub(crate) selected: &'a [usize],
}

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
    pub(crate) fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }

    pub(crate) fn sink_matches(&self, chain: Option<&str>, rooted: bool, argument: usize) -> bool {
        self.sinks.iter().any(|sink| {
            sink.member_calls
                .iter()
                .any(|member| chain == Some(member.as_str()))
                && sink.provenance.matches_rooted(rooted)
                && match &sink.args {
                    CompiledObjectSinkArgs::Any => true,
                    CompiledObjectSinkArgs::Indices(indices) => indices.contains(&argument),
                }
        })
    }

    pub(crate) fn requirements_ready(&self, completed: usize) -> bool {
        if self.all_requirements_required {
            completed == self.requirements.len()
        } else {
            completed != 0
        }
    }

    pub(crate) fn from_matcher(flow: &ObjectFlowMatcher) -> Self {
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
    pub(crate) member_call: String,
    pub(crate) arguments: Vec<ArgumentConstraint>,
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSource {
    fn from_matcher(source: &ObjectSourceMatcher) -> Self {
        Self {
            member_call: source.call.chain().to_string(),
            arguments: source.call.arguments().to_vec(),
            provenance: source.call.provenance.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledObjectRequirement {
    PropertyWrite {
        property: String,
        value: ValueMatcher,
    },
    MemberCall {
        member: String,
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
                member: member.clone(),
                arguments: arguments.clone(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CompiledObjectSinkArgs {
    Any,
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArgs {
    /// Return only sink argument positions that exist at this call site.
    ///
    /// Keeping the bounds check here makes callers unable to accidentally
    /// treat a rule's configured index as proof that the argument was passed.
    pub(crate) fn present_indices(&self, argument_count: usize) -> Vec<usize> {
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
    pub(crate) member_calls: Vec<String>,
    pub(crate) args: CompiledObjectSinkArgs,
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &FlowSinkMatcher) -> Self {
        match sink {
            FlowSinkMatcher::ArgumentOf { call, index } => Self {
                member_calls: vec![call.chain().to_string()],
                args: CompiledObjectSinkArgs::Indices(vec![*index]),
                provenance: call.provenance.clone(),
            },
            FlowSinkMatcher::AnyArgumentOf { call } => Self {
                member_calls: vec![call.chain().to_string()],
                args: CompiledObjectSinkArgs::Any,
                provenance: call.provenance.clone(),
            },
        }
    }
}

impl CompiledMatcherPlan {
    pub(crate) fn compile(matcher: &ApiMatcher) -> Self {
        Self {
            matcher: matcher.clone(),
            flows: matcher
                .flows
                .iter()
                .map(CompiledObjectFlow::from_matcher)
                .collect(),
        }
    }
}

impl<'a> CompiledMatcherCatalog<'a> {
    pub(crate) fn new(rules: &'a [CompiledRule], selected: &'a [usize]) -> Self {
        Self { rules, selected }
    }

    pub(crate) fn selected_matchers(&self) -> impl Iterator<Item = (usize, &CompiledMatcherPlan)> {
        self.selected
            .iter()
            .filter_map(move |&index| self.rules.get(index).map(|rule| (index, &rule.matcher)))
    }

    pub(crate) fn is_selected(&self, index: usize) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    pub(crate) fn get(&self, index: usize) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index).map(|rule| &rule.matcher)
    }

    pub(crate) fn len(&self) -> usize {
        self.rules.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRule {
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRule {
    pub(crate) fn new(rule: &Rule) -> Self {
        Self {
            matcher: CompiledMatcherPlan::compile(&ApiMatcher::from_matchers(
                rule.matchers().to_vec(),
            )),
        }
    }
}
