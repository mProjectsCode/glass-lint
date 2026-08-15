#[derive(Debug, Clone, Copy)]
pub(in crate::analysis) struct FlowLimits {
    objects: u32,
    states: usize,
    emissions: usize,
    mutation: usize,
    alternatives: usize,
    operations: usize,
}

const DEFAULT_OBJECTS: u64 = 65_536;
const DEFAULT_STATES: u64 = 262_144;
const DEFAULT_EMISSIONS: u64 = 65_536;
const DEFAULT_MUTATIONS: u64 = 4096;
const DEFAULT_FLOW_OPERATIONS: u64 = 262_144;
const MIN_OBJECTS: u32 = 1024;
const MIN_STATES: usize = 4096;
const MIN_EMISSIONS: usize = 1024;
const MIN_MUTATIONS: usize = 256;
const DEFAULT_ALTERNATIVES: usize = 4096;
const MIN_ALTERNATIVES: usize = 16;

impl FlowLimits {
    pub(in crate::analysis) fn from_flow_operations(flow_operations: usize) -> Self {
        Self {
            objects: u32::try_from(scaled_limit(
                DEFAULT_OBJECTS,
                flow_operations,
                MIN_OBJECTS as usize,
                u32::MAX as usize,
            ))
            .unwrap_or(u32::MAX),
            states: scaled_limit(DEFAULT_STATES, flow_operations, MIN_STATES, usize::MAX),
            emissions: scaled_limit(
                DEFAULT_EMISSIONS,
                flow_operations,
                MIN_EMISSIONS,
                usize::MAX,
            ),
            mutation: scaled_limit(
                DEFAULT_MUTATIONS,
                flow_operations,
                MIN_MUTATIONS,
                usize::MAX,
            ),
            alternatives: scaled_limit(
                DEFAULT_ALTERNATIVES as u64,
                flow_operations,
                MIN_ALTERNATIVES,
                usize::MAX,
            ),
            operations: flow_operations,
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

    pub(in crate::analysis) fn alternative_limit(&self) -> usize {
        self.alternatives
    }

    /// Maximum number of charged operations for one flow scope.
    pub(in crate::analysis) fn operation_limit(&self) -> usize {
        self.operations
    }

    #[cfg(test)]
    pub(in crate::analysis) fn test_new(
        objects: u32,
        states: usize,
        emissions: usize,
        mutation: usize,
    ) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
            alternatives: states.max(1),
            operations: usize::MAX,
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn test_with_operation_limit(
        objects: u32,
        states: usize,
        emissions: usize,
        mutation: usize,
        operations: usize,
    ) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
            alternatives: states.max(1),
            operations,
        }
    }
}

fn scaled_limit(default: u64, flow_operations: usize, minimum: usize, maximum: usize) -> usize {
    let flow = u64::try_from(flow_operations).unwrap_or(u64::MAX);
    let scaled = default
        .checked_mul(flow)
        .map_or(u64::MAX, |product| product / DEFAULT_FLOW_OPERATIONS);
    usize::try_from(scaled)
        .unwrap_or(usize::MAX)
        .clamp(minimum, maximum)
}
