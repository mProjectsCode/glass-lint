//! Pre-bound flow sources, requirements, and sinks.
//!
//! Constructed once per module between catalog compilation and flow
//! execution. Symbol paths are resolved to `NamePath` once so that
//! repeating `NamePath::from_symbol_path` calls during local and
//! cross-module projection are eliminated. Sources and sinks are indexed
//! by member-call chain for O(log n) lookup per chain instead of O(n)
//! per call.

use std::collections::BTreeMap;

use glass_lint_datastructures::{NamePath, NameTable};
use smol_str::SmolStr;

use crate::{
    analysis::model::flow::{FlowId, RequirementIndex, SinkIndex},
    api::{
        classification::RuleIndex,
        compiler::{CompiledObjectFlow, CompiledObjectRequirement},
        rule::query::lifecycle::LifecycleCallTarget,
    },
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum BoundLifecycleCallTarget {
    Member(NamePath),
    Global(SmolStr),
}

impl BoundLifecycleCallTarget {
    pub(super) fn from_lifecycle(target: &LifecycleCallTarget, names: &NameTable) -> Option<Self> {
        match target {
            LifecycleCallTarget::RootedMember(path) => names.lookup_path(path).map(Self::Member),
            LifecycleCallTarget::Global(name) => Some(Self::Global(name.clone())),
        }
    }

    fn member(path: NamePath) -> Self {
        Self::Member(path)
    }

    fn global(name: impl Into<SmolStr>) -> Self {
        Self::Global(name.into())
    }
}

#[derive(Clone, Debug)]
pub(super) struct BoundTargetIndex<T> {
    entries: BTreeMap<BoundLifecycleCallTarget, Vec<T>>,
}

impl<T> Default for BoundTargetIndex<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<T> BoundTargetIndex<T> {
    pub(super) fn insert(&mut self, target: BoundLifecycleCallTarget, value: T) {
        self.entries.entry(target).or_default().push(value);
    }

    pub(super) fn get(&self, target: &BoundLifecycleCallTarget) -> Option<&[T]> {
        self.entries.get(target).map(Vec::as_slice)
    }
}

impl<T: Ord> BoundTargetIndex<T> {
    pub(super) fn normalize(&mut self) {
        for values in self.entries.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPlan<'rules> {
    flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    sources: BoundTargetIndex<BoundSource>,
    sinks: BoundTargetIndex<FlowId>,
    /// Pre-resolved requirement member paths per flow, indexed by
    /// requirement position.  `None` for PropertyWrite requirements
    /// (which have no member-call path).
    req_members: BTreeMap<FlowId, Vec<Option<NamePath>>>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct BoundSource {
    pub(super) flow: FlowId,
    pub(super) arguments: Vec<crate::api::rule::ArgumentConstraint>,
}

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPaths {
    req_members: Vec<Option<NamePath>>,
}

impl BoundFlowPaths {
    pub(super) fn build(flow: &CompiledObjectFlow, names: &NameTable) -> Self {
        let req_members = flow
            .requirements
            .iter()
            .map(|req| match req {
                CompiledObjectRequirement::MemberCall { member, .. } => names.lookup_path(member),
                CompiledObjectRequirement::PropertyWrite { .. } => None,
            })
            .collect();
        Self { req_members }
    }

    pub(super) fn requirements_with_indices<'a>(
        &'a self,
        flow: &'a CompiledObjectFlow,
    ) -> impl Iterator<Item = (RequirementIndex, &'a CompiledObjectRequirement)> {
        flow.requirements
            .iter()
            .enumerate()
            .map(|(index, requirement)| (RequirementIndex::new(index), requirement))
    }

    pub(super) fn member_requirements<'a>(
        &'a self,
        flow: &'a CompiledObjectFlow,
    ) -> impl Iterator<
        Item = (
            RequirementIndex,
            &'a NamePath,
            &'a CompiledObjectRequirement,
        ),
    > {
        self.req_members
            .iter()
            .zip(flow.requirements.iter())
            .enumerate()
            .filter_map(|(index, (member, requirement))| {
                member
                    .as_ref()
                    .map(|member| (RequirementIndex::new(index), member, requirement))
            })
    }

    pub(super) fn matching_sink_indices(
        flow: &CompiledObjectFlow,
        argument_index: usize,
        mut target_matches: impl FnMut(&LifecycleCallTarget) -> bool,
    ) -> Vec<SinkIndex> {
        flow.sinks
            .iter()
            .enumerate()
            .filter_map(|(index, sink)| {
                let matches_arguments = match &sink.args {
                    crate::api::compiler::CompiledObjectSinkArguments::Any => true,
                    crate::api::compiler::CompiledObjectSinkArguments::Indices(indices) => {
                        indices.contains(&argument_index)
                    }
                };
                (target_matches(&sink.target) && matches_arguments).then_some(SinkIndex::new(index))
            })
            .collect()
    }
}

impl<'rules> BoundFlowPlan<'rules> {
    /// Build a plan from compiled flow matchers.
    pub(super) fn new(
        rules: &[(RuleIndex, usize, &'rules CompiledObjectFlow)],
        names: &NameTable,
    ) -> Self {
        let mut flows = BTreeMap::new();
        let mut sources = BoundTargetIndex::default();
        let mut sinks = BoundTargetIndex::default();
        let mut req_members = BTreeMap::new();

        for (rule_index, flow_index, flow) in rules {
            let id = FlowId::new(*rule_index, *flow_index);
            flows.insert(id, *flow);

            for source in &flow.sources {
                let bound = BoundSource {
                    flow: id,
                    arguments: source.arguments.clone(),
                };
                if let Some(target) =
                    BoundLifecycleCallTarget::from_lifecycle(&source.target, names)
                {
                    sources.insert(target, bound);
                }
            }

            for sink in &flow.sinks {
                if let Some(target) = BoundLifecycleCallTarget::from_lifecycle(&sink.target, names)
                {
                    sinks.insert(target, id);
                }
            }

            let paths = BoundFlowPaths::build(flow, names);
            req_members.insert(id, paths.req_members);
        }

        sources.normalize();
        sinks.normalize();

        Self {
            flows,
            sources,
            sinks,
            req_members,
        }
    }

    /// Look up a compiled flow by its stable identifier.
    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    /// Look up executable source candidates by their bound member chain.
    pub(super) fn source_candidates(&self, member_call: &NamePath) -> Option<&[BoundSource]> {
        self.sources
            .get(&BoundLifecycleCallTarget::member(member_call.clone()))
    }

    pub(super) fn global_source_candidates(&self, name: &str) -> Option<&[BoundSource]> {
        self.sources.get(&BoundLifecycleCallTarget::global(name))
    }

    /// Look up flows whose sink chain matches `member_call`.
    pub(super) fn sink_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sinks
            .get(&BoundLifecycleCallTarget::member(member_call.clone()))
    }

    pub(super) fn global_sink_ids(&self, name: &str) -> Option<&[FlowId]> {
        self.sinks.get(&BoundLifecycleCallTarget::global(name))
    }

    pub(super) fn requirements_with_indices(
        &self,
        flow_id: FlowId,
    ) -> impl Iterator<Item = (RequirementIndex, &CompiledObjectRequirement)> {
        self.get(flow_id)
            .into_iter()
            .flat_map(|flow| flow.requirements.iter().enumerate())
            .map(|(index, requirement)| (RequirementIndex::new(index), requirement))
    }

    pub(super) fn member_requirements(
        &self,
        flow_id: FlowId,
    ) -> impl Iterator<Item = (RequirementIndex, &NamePath, &CompiledObjectRequirement)> {
        self.get(flow_id)
            .into_iter()
            .zip(self.req_members.get(&flow_id))
            .flat_map(|(flow, members)| {
                members
                    .iter()
                    .zip(flow.requirements.iter())
                    .enumerate()
                    .filter_map(|(index, (member, requirement))| {
                        member
                            .as_ref()
                            .map(|member| (RequirementIndex::new(index), member, requirement))
                    })
            })
    }

    pub(super) fn matching_sink_indices(
        &self,
        flow_id: FlowId,
        argument_index: usize,
        target_matches: impl FnMut(&LifecycleCallTarget) -> bool,
    ) -> Vec<SinkIndex> {
        self.get(flow_id).map_or_else(Vec::new, |flow| {
            BoundFlowPaths::matching_sink_indices(flow, argument_index, target_matches)
        })
    }
}
