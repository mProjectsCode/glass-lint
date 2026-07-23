//! Compiled source and sink lookup for declarative flow matchers.
//!
//! This index is independent of the mutable object-flow projector.  It owns the
//! rule-facing lookup keys while the collector owns object identity, state,
//! and lifecycle transitions.

use std::collections::BTreeMap;

use crate::{
    analysis::{name::NameTable, value::NamePath},
    api::{classification::RuleIndex, compiler::CompiledObjectFlow},
};

/// Default per-dimension budgets that match the prior hard-coded constants.
const DEFAULT_OBJECTS: u64 = 65_536;
const DEFAULT_STATES: u64 = 262_144;
const DEFAULT_EMISSIONS: u64 = 65_536;
const DEFAULT_MUTATIONS: u64 = 4096;

/// Floor values that guarantee even the simplest local flow can complete.
/// These are separate from the cross‑module `flow_operations` budget.
const MIN_OBJECTS: u32 = 1024;
const MIN_STATES: usize = 4096;
const MIN_EMISSIONS: usize = 1024;
const MIN_MUTATIONS: usize = 256;

#[derive(Debug, Clone, Copy)]
/// Bounded limits for object-flow identities, states, emissions, and mutation
/// log. Budgets are derived from the validated `flow_operations` limit by
/// scaling the defaults proportionally, with generous floors so that a single
/// local function always has enough capacity regardless of the cross‑module
/// budget.
pub(in crate::analysis) struct FlowLimits {
    objects: u32,
    states: usize,
    emissions: usize,
    mutation: usize,
}

impl FlowLimits {
    /// Scale each dimension from its default proportionally to
    /// `flow_operations`, clamped to a generous floor so that a single local
    /// function always has enough capacity.
    #[allow(clippy::cast_possible_truncation)]
    pub(in crate::analysis) fn from_flow_operations(flow_operations: usize) -> Self {
        let flow = flow_operations as u64;
        Self {
            objects: u32::try_from(DEFAULT_OBJECTS * flow / 262_144)
                .unwrap_or(u32::MAX)
                .max(MIN_OBJECTS),
            states: ((DEFAULT_STATES * flow / 262_144) as usize).max(MIN_STATES),
            emissions: ((DEFAULT_EMISSIONS * flow / 262_144) as usize).max(MIN_EMISSIONS),
            mutation: ((DEFAULT_MUTATIONS * flow / 262_144) as usize).max(MIN_MUTATIONS),
        }
    }

    pub(in crate::analysis) fn object_limit(&self) -> u32 {
        self.objects
    }

    pub(in crate::analysis) fn state_limit(&self) -> usize {
        self.states
    }

    pub(in crate::analysis) fn emission_limit(&self) -> usize {
        self.emissions
    }

    pub(in crate::analysis) fn mutation_limit(&self) -> usize {
        self.mutation
    }

    /// Test-only: construct a `FlowLimits` with explicit per-dimension values.
    #[cfg(test)]
    pub(super) fn test_new(objects: u32, states: usize, emissions: usize, mutation: usize) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identifier for one selected rule flow matcher.
pub(super) struct FlowId {
    rule_index: RuleIndex,
    flow_index: usize,
}

impl FlowId {
    pub(super) fn new(rule_index: RuleIndex, flow_index: usize) -> Self {
        Self {
            rule_index,
            flow_index,
        }
    }

    pub(super) fn rule_index(self) -> RuleIndex {
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
    sources: BTreeMap<NamePath, Vec<FlowId>>,
    sinks: BTreeMap<NamePath, Vec<FlowId>>,
}

impl<'rules> FlowIndex<'rules> {
    pub(super) fn new(
        rules: &[(RuleIndex, usize, &'rules CompiledObjectFlow)],
        names: &NameTable,
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
                if let Some(member_call) = NamePath::from_symbol_path(&source.member_call, names) {
                    index.add_source(&member_call, id);
                }
            }
            for sink in &flow.sinks {
                for member_call in &sink.member_calls {
                    if let Some(member_call) = NamePath::from_symbol_path(member_call, names) {
                        index.add_sink(&member_call, id);
                    }
                }
            }
        }
        index.normalize_ids();
        index
    }

    pub(super) fn get(&self, id: FlowId) -> Option<&CompiledObjectFlow> {
        self.flows.get(&id).copied()
    }

    pub(super) fn source_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sources.get(member_call).map(Vec::as_slice)
    }

    pub(super) fn sink_ids(&self, member_call: &NamePath) -> Option<&[FlowId]> {
        self.sinks.get(member_call).map(Vec::as_slice)
    }

    fn add_source(&mut self, member_call: &NamePath, id: FlowId) {
        self.sources
            .entry(member_call.clone())
            .or_default()
            .push(id);
    }

    fn add_sink(&mut self, member_call: &NamePath, id: FlowId) {
        self.sinks.entry(member_call.clone()).or_default().push(id);
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
