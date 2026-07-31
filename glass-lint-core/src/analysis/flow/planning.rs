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
    analysis::model::flow::FlowId,
    api::{
        classification::RuleIndex,
        compiler::{CompiledObjectFlow, CompiledObjectRequirement},
        rule::query::lifecycle::LifecycleCallTarget,
    },
};

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPlan<'rules> {
    flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    sources: BTreeMap<NamePath, Vec<BoundSource>>,
    global_sources: BTreeMap<SmolStr, Vec<BoundSource>>,
    sinks: BTreeMap<NamePath, Vec<FlowId>>,
    global_sinks: BTreeMap<SmolStr, Vec<FlowId>>,
    /// Pre-resolved requirement member paths per flow, indexed by
    /// requirement position.  `None` for PropertyWrite requirements
    /// (which have no member-call path).
    req_members: BTreeMap<FlowId, Vec<Option<NamePath>>>,
}

#[derive(Debug, Clone)]
pub(super) struct BoundSource {
    pub(super) flow: FlowId,
    pub(super) arguments: Vec<crate::api::rule::ArgumentConstraint>,
}

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPaths {
    pub(super) req_members: Vec<Option<NamePath>>,
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
}

impl<'rules> BoundFlowPlan<'rules> {
    /// Build a plan from compiled flow matchers.
    pub(super) fn new(
        rules: &[(RuleIndex, usize, &'rules CompiledObjectFlow)],
        names: &NameTable,
    ) -> Self {
        let mut flows = BTreeMap::new();
        let mut sources: BTreeMap<NamePath, Vec<BoundSource>> = BTreeMap::new();
        let mut global_sources: BTreeMap<SmolStr, Vec<BoundSource>> = BTreeMap::new();
        let mut sinks: BTreeMap<NamePath, Vec<FlowId>> = BTreeMap::new();
        let mut global_sinks: BTreeMap<SmolStr, Vec<FlowId>> = BTreeMap::new();
        let mut req_members = BTreeMap::new();

        for (rule_index, flow_index, flow) in rules {
            let id = FlowId::new(*rule_index, *flow_index);
            flows.insert(id, *flow);

            for source in &flow.sources {
                let bound = BoundSource {
                    flow: id,
                    arguments: source.arguments.clone(),
                };
                match &source.target {
                    LifecycleCallTarget::RootedMember(member) => {
                        if let Some(member) = names.lookup_path(member) {
                            sources.entry(member).or_default().push(bound);
                        }
                    }
                    LifecycleCallTarget::Global(name) => {
                        global_sources.entry(name.clone()).or_default().push(bound);
                    }
                }
            }

            for sink in &flow.sinks {
                match &sink.target {
                    LifecycleCallTarget::RootedMember(path) => {
                        if let Some(member) = names.lookup_path(path) {
                            sinks.entry(member).or_default().push(id);
                        }
                    }
                    LifecycleCallTarget::Global(name) => {
                        global_sinks.entry(name.clone()).or_default().push(id);
                    }
                }
            }

            let paths = BoundFlowPaths::build(flow, names);
            req_members.insert(id, paths.req_members);
        }

        for candidates in sources.values_mut() {
            candidates.sort_by(|left, right| {
                left.flow
                    .cmp(&right.flow)
                    .then_with(|| left.arguments.cmp(&right.arguments))
            });
            candidates.dedup_by(|left, right| {
                left.flow == right.flow && left.arguments == right.arguments
            });
        }
        for candidates in global_sources.values_mut() {
            candidates.sort_by(|left, right| {
                left.flow
                    .cmp(&right.flow)
                    .then_with(|| left.arguments.cmp(&right.arguments))
            });
            candidates.dedup_by(|left, right| {
                left.flow == right.flow && left.arguments == right.arguments
            });
        }
        for ids in sinks.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
        for ids in global_sinks.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }

        Self {
            flows,
            sources,
            global_sources,
            sinks,
            global_sinks,
            req_members,
        }
    }

    /// Look up a compiled flow by its stable identifier.
    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    /// Look up executable source candidates by their bound member chain.
    pub(super) fn source_candidates(&self, member_call: &NamePath) -> Option<&[BoundSource]> {
        self.sources.get(member_call).map(Vec::as_slice)
    }

    pub(super) fn global_source_candidates(&self, name: &str) -> Option<&[BoundSource]> {
        self.global_sources.get(name).map(Vec::as_slice)
    }

    /// Look up flows whose sink chain matches `member_call`.
    pub(super) fn sink_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sinks.get(member_call).map(Vec::as_slice)
    }

    pub(super) fn global_sink_ids(&self, name: &str) -> Option<&[FlowId]> {
        self.global_sinks.get(name).map(Vec::as_slice)
    }

    /// Pre-resolved requirement member paths for `flow_id`.
    ///
    /// Each entry is `Some(NamePath)` for a MemberCall requirement or
    /// `None` for a PropertyWrite requirement.  Returns an empty slice
    /// when `flow_id` is not in the plan.
    pub(super) fn requirement_members(&self, flow_id: FlowId) -> &[Option<NamePath>] {
        self.req_members.get(&flow_id).map_or(&[], Vec::as_slice)
    }
}
