use super::super::rule::{
    ApiMatcher, ArgumentConstraint, FlowCompletion, FlowCondition, FlowSinkMatcher,
    MemberCallProvenance, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
    ValueMatcher,
};

/// Canonical matcher representation consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
#[derive(Debug, Clone)]
pub struct CompiledMatcherPlan {
    pub matcher: ApiMatcher,
    pub flows: Vec<CompiledObjectFlow>,
}

#[derive(Debug, Clone)]
pub struct CompiledMatcherCatalog<'a> {
    pub rules: &'a [CompiledRule],
    pub selected: &'a [usize],
}

#[derive(Debug, Clone)]
pub struct CompiledObjectFlow {
    pub symbol: String,
    pub sources: Vec<CompiledObjectSource>,
    pub requirements: Vec<CompiledObjectRequirement>,
    pub sinks: Vec<CompiledObjectSink>,
    pub all_requirements_required: bool,
    pub emit_on_requirements: bool,
}

impl CompiledObjectFlow {
    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }

    pub fn sink_matches(&self, chain: Option<&str>, rooted: bool, argument: usize) -> bool {
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
pub struct CompiledObjectSource {
    pub member_call: String,
    pub arguments: Vec<ArgumentConstraint>,
    pub provenance: MemberCallProvenance,
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
pub enum CompiledObjectRequirement {
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
pub enum CompiledObjectSinkArgs {
    Any,
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArgs {
    /// Return only sink argument positions that exist at this call site.
    ///
    /// Keeping the bounds check here makes callers unable to accidentally
    /// treat a rule's configured index as proof that the argument was passed.
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
pub struct CompiledObjectSink {
    pub member_calls: Vec<String>,
    pub args: CompiledObjectSinkArgs,
    pub provenance: MemberCallProvenance,
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
    pub fn compile(matcher: &ApiMatcher) -> Self {
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
    pub fn new(rules: &'a [CompiledRule], selected: &'a [usize]) -> Self {
        Self { rules, selected }
    }

    pub fn selected_matchers(&self) -> impl Iterator<Item = (usize, &CompiledMatcherPlan)> {
        self.selected
            .iter()
            .filter_map(move |&index| self.rules.get(index).map(|rule| (index, &rule.matcher)))
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    pub fn get(&self, index: usize) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index).map(|rule| &rule.matcher)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub matcher: CompiledMatcherPlan,
}

impl CompiledRule {
    pub fn new(rule: &Rule) -> Self {
        Self {
            matcher: CompiledMatcherPlan::compile(&ApiMatcher::from_matchers(
                rule.matchers().to_vec(),
            )),
        }
    }
}
