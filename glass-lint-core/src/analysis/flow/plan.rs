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

use super::index::FlowId;
use crate::api::{
    classification::RuleIndex,
    compiler::{CompiledObjectFlow, CompiledObjectRequirement},
};

#[derive(Debug, Clone)]
pub(super) struct BoundFlowPlan<'rules> {
    flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    sources: BTreeMap<NamePath, Vec<FlowId>>,
    sinks: BTreeMap<NamePath, Vec<FlowId>>,
    /// Pre-resolved requirement member paths per flow, indexed by
    /// requirement position.  `None` for PropertyWrite requirements
    /// (which have no member-call path).
    req_members: BTreeMap<FlowId, Vec<Option<NamePath>>>,
    /// Pre-resolved sink member-call paths per flow, indexed by sink
    /// position.  Each entry lists every member-call chain that the
    /// compiled sink matches.
    sink_members: BTreeMap<FlowId, Vec<Vec<NamePath>>>,
}

impl<'rules> BoundFlowPlan<'rules> {
    /// Build a plan from compiled flow matchers.
    pub(super) fn new(
        rules: &[(RuleIndex, usize, &'rules CompiledObjectFlow)],
        names: &NameTable,
    ) -> Self {
        let mut flows = BTreeMap::new();
        let mut sources: BTreeMap<NamePath, Vec<FlowId>> = BTreeMap::new();
        let mut sinks: BTreeMap<NamePath, Vec<FlowId>> = BTreeMap::new();
        let mut req_members = BTreeMap::new();
        let mut sink_members = BTreeMap::new();

        for (rule_index, flow_index, flow) in rules {
            let id = FlowId::new(*rule_index, *flow_index);
            flows.insert(id, *flow);

            for source in &flow.sources {
                if let Some(member) = names.lookup_path(&source.member_call) {
                    sources.entry(member).or_default().push(id);
                }
            }

            for sink in &flow.sinks {
                for member in &sink.member_calls {
                    if let Some(member) = names.lookup_path(member) {
                        sinks.entry(member).or_default().push(id);
                    }
                }
            }

            let reqs: Vec<Option<NamePath>> = flow
                .requirements
                .iter()
                .map(|req| match req {
                    CompiledObjectRequirement::MemberCall { member, .. } => {
                        names.lookup_path(member)
                    }
                    CompiledObjectRequirement::PropertyWrite { .. } => None,
                })
                .collect();
            req_members.insert(id, reqs);

            let sink_data: Vec<Vec<NamePath>> = flow
                .sinks
                .iter()
                .map(|sink| {
                    sink.member_calls
                        .iter()
                        .filter_map(|mc| names.lookup_path(mc))
                        .collect()
                })
                .collect();
            sink_members.insert(id, sink_data);
        }

        for ids in sources.values_mut().chain(sinks.values_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }

        Self {
            flows,
            sources,
            sinks,
            req_members,
            sink_members,
        }
    }

    /// Look up a compiled flow by its stable identifier.
    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    /// Look up flows whose source chain matches `member_call`.
    pub(super) fn source_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sources.get(member_call).map(Vec::as_slice)
    }

    /// Look up flows whose sink chain matches `member_call`.
    pub(super) fn sink_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sinks.get(member_call).map(Vec::as_slice)
    }

    /// Pre-resolved requirement member paths for `flow_id`.
    ///
    /// Each entry is `Some(NamePath)` for a MemberCall requirement or
    /// `None` for a PropertyWrite requirement.  Returns an empty slice
    /// when `flow_id` is not in the plan.
    pub(super) fn requirement_members(&self, flow_id: FlowId) -> &[Option<NamePath>] {
        self.req_members.get(&flow_id).map_or(&[], Vec::as_slice)
    }

    /// Pre-resolved sink member-call paths for `flow_id`.
    ///
    /// Each entry lists every member-call chain that the corresponding
    /// compiled sink matches.  Returns an empty slice when `flow_id` is
    /// not in the plan.
    pub(super) fn sink_member_calls(&self, flow_id: FlowId) -> &[Vec<NamePath>] {
        self.sink_members.get(&flow_id).map_or(&[], Vec::as_slice)
    }
}
