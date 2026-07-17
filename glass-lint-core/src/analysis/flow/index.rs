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
/// Hard limits for object-flow identities, states, and emissions.
pub(super) struct FlowLimits {
    /// Maximum object identities.
    objects: u32,
    /// Maximum projected states.
    states: usize,
    /// Maximum evidence emissions.
    emissions: usize,
}

impl FlowLimits {
    pub(super) fn object_limit(&self) -> u32 {
        self.objects
    }

    pub(super) fn state_limit(&self) -> usize {
        self.states
    }

    pub(super) fn emission_limit(&self) -> usize {
        self.emissions
    }
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
/// Stable identifier for one selected rule flow matcher.
pub(super) struct FlowId {
    rule_index: crate::api::classification::RuleIndex,
    flow_index: usize,
}

impl FlowId {
    pub(super) fn new(
        rule_index: crate::api::classification::RuleIndex,
        flow_index: usize,
    ) -> Self {
        Self {
            rule_index,
            flow_index,
        }
    }

    pub(super) fn rule_index(self) -> crate::api::classification::RuleIndex {
        self.rule_index
    }

    pub(super) fn flow_index(self) -> usize {
        self.flow_index
    }
}

#[derive(Debug, Default, Clone)]
/// Rule-facing source/sink lookup buckets for selected flow matchers.
pub(super) struct FlowIndex<'rules> {
    flows: BTreeMap<FlowId, &'rules CompiledObjectFlow>,
    sources: BTreeMap<String, Vec<FlowId>>,
    sinks: BTreeMap<String, Vec<FlowId>>,
}

impl<'rules> FlowIndex<'rules> {
    pub(super) fn new(
        rules: &[(
            crate::api::classification::RuleIndex,
            usize,
            &'rules CompiledObjectFlow,
        )],
    ) -> Self {
        // BTreeMap-backed keys make matcher lookup and emission deterministic
        // regardless of catalog construction order.
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

    pub(super) fn source_ids(&self, member_call: &str) -> Option<&[FlowId]> {
        self.sources.get(member_call).map(Vec::as_slice)
    }

    pub(super) fn sink_ids(&self, member_call: &str) -> Option<&[FlowId]> {
        self.sinks.get(member_call).map(Vec::as_slice)
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
