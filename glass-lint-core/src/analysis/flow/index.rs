//! Flow identity, limits, and stable identifiers for declarative flow matchers.

use crate::api::classification::RuleIndex;

/// Default per-dimension budgets that match the prior hard-coded constants.
const DEFAULT_OBJECTS: u64 = 65_536;
const DEFAULT_STATES: u64 = 262_144;
const DEFAULT_EMISSIONS: u64 = 65_536;
const DEFAULT_MUTATIONS: u64 = 4096;
/// Flow-operations denominator used to scale each dimension proportionally.
const DEFAULT_FLOW_OPERATIONS: u64 = 262_144;

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
            objects: u32::try_from(DEFAULT_OBJECTS * flow / DEFAULT_FLOW_OPERATIONS)
                .unwrap_or(u32::MAX)
                .max(MIN_OBJECTS),
            states: ((DEFAULT_STATES * flow / DEFAULT_FLOW_OPERATIONS) as usize).max(MIN_STATES),
            emissions: ((DEFAULT_EMISSIONS * flow / DEFAULT_FLOW_OPERATIONS) as usize)
                .max(MIN_EMISSIONS),
            mutation: ((DEFAULT_MUTATIONS * flow / DEFAULT_FLOW_OPERATIONS) as usize)
                .max(MIN_MUTATIONS),
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
