//! Compiled declarative matcher plans and object-flow projections.
//!
//! The compiler preserves matcher semantics in owned, immutable structures.
//! Selection only filters catalog indexes; it never changes the semantic facts
//! constructed for a source file.

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
    /// Direct API matcher predicates.
    pub matcher: ApiMatcher,
    /// Object-flow configurations derived from the public matcher.
    pub flows: Vec<CompiledObjectFlow>,
}

#[derive(Debug, Clone)]
/// Borrowed view of compiled rules selected for a classification run.
pub struct CompiledMatcherCatalog<'a> {
    /// All compiled rules, retained for stable rule indexes.
    pub rules: &'a [CompiledRule],
    /// Sorted selected rule indexes.
    pub selected: &'a [usize],
}

#[derive(Debug, Clone)]
/// Compiled source/requirement/sink flow configuration for one symbol.
pub struct CompiledObjectFlow {
    /// Evidence symbol emitted for this flow.
    pub symbol: String,
    /// Object-producing member-call sources.
    pub sources: Vec<CompiledObjectSource>,
    /// Required object events.
    pub requirements: Vec<CompiledObjectRequirement>,
    /// Terminal sink patterns.
    pub sinks: Vec<CompiledObjectSink>,
    /// Whether every configured requirement must be observed.
    pub all_requirements_required: bool,
    /// Whether configuration itself emits evidence after requirements.
    pub emit_on_requirements: bool,
}

impl CompiledObjectFlow {
    /// Return the flow's stable evidence symbol.
    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }

    /// Test a sink chain, provenance mode, and argument position.
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

    /// Whether the observed requirement count satisfies this flow condition.
    pub fn requirements_ready(&self, completed: usize) -> bool {
        if self.all_requirements_required {
            completed == self.requirements.len()
        } else {
            completed != 0
        }
    }

    /// Compile one public object-flow matcher into owned plan data.
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
/// Compiled member-call source constraint.
pub struct CompiledObjectSource {
    /// Required member-call chain.
    pub member_call: String,
    /// Argument constraints on the source call.
    pub arguments: Vec<ArgumentConstraint>,
    /// Required rooted/module provenance mode.
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
/// Event that must be observed on a flowed object.
pub enum CompiledObjectRequirement {
    /// Required property write and value constraint.
    PropertyWrite {
        /// Written property name.
        property: String,
        /// Required value matcher.
        value: ValueMatcher,
    },
    /// Required member call and argument constraints.
    MemberCall {
        /// Required member-call name.
        member: String,
        /// Argument constraints for the call.
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
/// Argument-position matching mode for a compiled sink.
pub enum CompiledObjectSinkArgs {
    /// Match every argument position present at the call site.
    Any,
    /// Match only configured argument positions.
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
/// Compiled terminal sink pattern for object flow.
pub struct CompiledObjectSink {
    /// Accepted sink member-call chains.
    pub member_calls: Vec<String>,
    /// Accepted argument-position mode.
    pub args: CompiledObjectSinkArgs,
    /// Required rooted/module provenance mode.
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
    /// Compile a public API matcher and all of its object flows.
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
    /// Create a borrowed catalog view over sorted selected indexes.
    pub fn new(rules: &'a [CompiledRule], selected: &'a [usize]) -> Self {
        Self { rules, selected }
    }

    /// Iterate selected plans while preserving their catalog indexes.
    pub fn selected_matchers(&self) -> impl Iterator<Item = (usize, &CompiledMatcherPlan)> {
        self.selected
            .iter()
            .filter_map(move |&index| self.rules.get(index).map(|rule| (index, &rule.matcher)))
    }

    /// Whether a catalog index is selected by this view.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    /// Borrow a compiled plan by its stable catalog index.
    pub fn get(&self, index: usize) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index).map(|rule| &rule.matcher)
    }

    /// Return the total catalog rule count.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[derive(Debug, Clone)]
/// One public rule paired with its compiled matcher plan.
pub struct CompiledRule {
    /// Compiled matcher data for the rule.
    pub matcher: CompiledMatcherPlan,
}

impl CompiledRule {
    /// Compile a rule's declared matcher list into one canonical plan.
    pub fn new(rule: &Rule) -> Self {
        Self {
            matcher: CompiledMatcherPlan::compile(&ApiMatcher::from_matchers(
                rule.matchers().to_vec(),
            )),
        }
    }
}
