//! Compiled source and sink lookup for declarative flow matchers.
//!
//! This index is independent of the mutable object-flow projector.  It owns the
//! rule-facing lookup keys while the collector owns object identity, state,
//! and lifecycle transitions.

use std::collections::BTreeMap;

use crate::api::compiler::CompiledObjectFlow;

const MAX_FLOW_OBJECTS: u32 = 65_536;
const MAX_FLOW_STATES: usize = 262_144;
const MAX_FLOW_EMISSIONS: usize = 65_536;

#[derive(Debug, Clone, Copy)]
pub(super) struct FlowLimits {
    pub(super) objects: u32,
    pub(super) states: usize,
    pub(super) emissions: usize,
}

impl Default for FlowLimits {
    fn default() -> Self {
        Self {
            objects: MAX_FLOW_OBJECTS,
            states: MAX_FLOW_STATES,
            emissions: MAX_FLOW_EMISSIONS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct FlowId {
    pub(super) rule_index: usize,
    pub(super) flow_index: usize,
}

#[derive(Debug, Default, Clone)]
pub(super) struct FlowIndex<'rules> {
    pub(super) flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    pub(super) sources: BTreeMap<String, Vec<FlowId>>,
    pub(super) sinks: BTreeMap<String, Vec<FlowId>>,
}

impl<'rules> FlowIndex<'rules> {
    pub(super) fn new(rules: &[(usize, usize, &'rules CompiledObjectFlow)]) -> Self {
        let mut index = Self::default();
        for (rule_index, flow_index, flow) in rules {
            let id = FlowId {
                rule_index: *rule_index,
                flow_index: *flow_index,
            };
            index.flows.insert(id, *flow);
            for source in &flow.sources {
                index.add_source(&source.member_call, id);
            }
            for sink in &flow.sinks {
                for member_call in &sink.member_calls {
                    index.add_sink(member_call, id);
                }
            }
        }
        index.normalize_ids();
        index
    }

    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    fn add_source(&mut self, member_call: &str, id: FlowId) {
        self.sources
            .entry(member_call.to_string())
            .or_default()
            .push(id);
    }

    fn add_sink(&mut self, member_call: &str, id: FlowId) {
        self.sinks
            .entry(member_call.to_string())
            .or_default()
            .push(id);
    }

    /// Normalize lookup buckets once after construction. Query code can then
    /// treat every bucket as a deterministic set without repeating dedup work.
    fn normalize_ids(&mut self) {
        for ids in self.sources.values_mut().chain(self.sinks.values_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }
    }
}
