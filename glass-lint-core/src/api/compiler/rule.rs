use std::collections::BTreeSet;

#[cfg(test)]
use super::super::rule::FlowMatcher;
use super::super::rule::{
    ApiMatcher, ApiRule, ArgumentConstraint, FlowCompletion, FlowCondition, FlowSinkMatcher,
    MemberCallProvenance, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ValueMatcher,
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
    pub(crate) matchers: Vec<&'a CompiledMatcherPlan>,
    pub(crate) selected: &'a BTreeSet<usize>,
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
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledObjectSource {
    pub(crate) member_call: String,
    pub(crate) arguments: Vec<ArgumentConstraint>,
    pub(crate) provenance: MemberCallProvenance,
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

#[derive(Debug, Clone)]
pub(crate) enum CompiledObjectSinkArgs {
    Any,
    Indices(Vec<usize>),
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledObjectSink {
    pub(crate) member_calls: Vec<String>,
    pub(crate) args: CompiledObjectSinkArgs,
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledMatcherPlan {
    pub(crate) fn compile(matcher: &ApiMatcher) -> Self {
        Self {
            matcher: matcher.clone(),
            flows: matcher.flows.iter().map(compile_flow).collect(),
        }
    }
}

impl<'a> CompiledMatcherCatalog<'a> {
    pub(crate) fn new(
        matchers: Vec<&'a CompiledMatcherPlan>,
        selected: &'a BTreeSet<usize>,
    ) -> Self {
        Self { matchers, selected }
    }

    pub(crate) fn selected_matchers(&self) -> impl Iterator<Item = (usize, &CompiledMatcherPlan)> {
        self.matchers
            .iter()
            .enumerate()
            .filter_map(move |(index, matcher)| {
                self.selected.contains(&index).then_some((index, *matcher))
            })
    }

    pub(crate) fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    pub(crate) fn get(&self, index: usize) -> Option<&'a CompiledMatcherPlan> {
        self.matchers.get(index).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.matchers.len()
    }
}

fn compile_flow(flow: &ObjectFlowMatcher) -> CompiledObjectFlow {
    let (requirements, all_requirements_required) = match flow.condition.as_ref() {
        Some(FlowCondition::AnyOf(events)) => (events.iter().map(compile_event).collect(), false),
        Some(FlowCondition::AllOf(events)) => (events.iter().map(compile_event).collect(), true),
        None => (Vec::new(), false),
    };
    let (sinks, emit_on_requirements) = match flow.completion.as_ref() {
        Some(FlowCompletion::Configuration) => (Vec::new(), true),
        Some(FlowCompletion::AnySink(sinks)) => (sinks.iter().map(compile_sink).collect(), false),
        None => (Vec::new(), false),
    };
    CompiledObjectFlow {
        symbol: flow.symbol.clone(),
        sources: flow.sources.iter().map(compile_source).collect(),
        requirements,
        sinks,
        all_requirements_required,
        emit_on_requirements,
    }
}

#[cfg(test)]
pub(crate) fn compile_legacy_flow(flow: FlowMatcher) -> CompiledObjectFlow {
    let flow: ObjectFlowMatcher = flow.into();
    compile_flow(&flow)
}

fn compile_source(source: &ObjectSourceMatcher) -> CompiledObjectSource {
    CompiledObjectSource {
        member_call: source.call.chain().to_string(),
        arguments: source.call.arguments().to_vec(),
        provenance: source.call.provenance.clone(),
    }
}

fn compile_event(event: &ObjectEventMatcher) -> CompiledObjectRequirement {
    match event {
        ObjectEventMatcher::PropertyWrite { property, value } => {
            CompiledObjectRequirement::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            }
        }
        ObjectEventMatcher::MemberCall { member, arguments } => {
            CompiledObjectRequirement::MemberCall {
                member: member.clone(),
                arguments: arguments.clone(),
            }
        }
    }
}

fn compile_sink(sink: &FlowSinkMatcher) -> CompiledObjectSink {
    match sink {
        FlowSinkMatcher::ArgumentOf { call, index } => CompiledObjectSink {
            member_calls: vec![call.chain().to_string()],
            args: CompiledObjectSinkArgs::Indices(vec![*index]),
            provenance: call.provenance.clone(),
        },
        FlowSinkMatcher::AnyArgumentOf { call } => CompiledObjectSink {
            member_calls: vec![call.chain().to_string()],
            args: CompiledObjectSinkArgs::Any,
            provenance: call.provenance.clone(),
        },
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRule {
    #[allow(dead_code)]
    pub(crate) catalog_index: usize,
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRule {
    pub(crate) fn new(catalog_index: usize, rule: &ApiRule) -> Self {
        Self {
            catalog_index,
            matcher: rule.matcher_for_compilation(),
        }
    }
}
