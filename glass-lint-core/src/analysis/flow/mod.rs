//! Bounded semantic flow projection over the immutable fact stream.
//!
//! Local effects and indexes are built once from facts during semantic
//! analysis; matcher-specific projection follows only proven identities and
//! bounded state. The cross-module overlay composes per-function summaries
//! without re-traversing syntax or retaining caller state.
//!
//! Projection is bounded by `AnalysisLimits::flow_operations` and records
//! exhaustion as an `IncompleteReason::BudgetExhausted` status entry rather
//! than synthesizing partial flow state.

pub(super) mod cross;
pub mod effect;
pub(super) mod matcher;
pub(super) mod planning;
pub(super) mod projector;
pub(super) mod summary;

/// Bounded completion state shared by flow phases.
///
/// A phase is complete only when no reason bit is set. Multiple bounded
/// resources may exhaust during one phase, so merging preserves every reason
/// instead of collapsing the result to a boolean at a phase boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct FlowCompletion(u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FlowCompletionReason {
    Summary = 0,
    ObjectLimit,
    StateLimit,
    EvidenceLimit,
    MutationLog,
    Alternatives,
    TraceArena,
    EffectBudget,
    SummaryBudget,
    SummarySinkCapacity,
    SummaryWorklistCapacity,
    SourcePropagation,
    CrossStepBudget,
    CrossContextLimit,
}

impl FlowCompletion {
    pub(super) fn incomplete(reason: FlowCompletionReason) -> Self {
        let mut completion = Self::default();
        completion.mark(reason);
        completion
    }

    pub(super) fn mark(&mut self, reason: FlowCompletionReason) {
        self.0 |= 1 << reason as u8;
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub(super) fn is_complete(self) -> bool {
        self.0 == 0
    }

    pub(super) fn is_incomplete(self) -> bool {
        !self.is_complete()
    }
}

#[cfg(test)]
mod tests;
