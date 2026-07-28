use crate::{
    analysis::{
        facts::FactId,
        flow::cross::MAX_SOURCE_REFINEMENT_ROUNDS,
        model::flow::{FlowId, RequirementSet},
        value::{FunctionId, ValueId},
    },
    project::ModuleId,
};

#[derive(Debug)]
/// Fixed-point budget for propagating source identities through helper calls.
pub(super) struct SourceBudget {
    rounds: usize,
}

impl SourceBudget {
    pub(super) fn new() -> Self {
        Self { rounds: 0 }
    }

    pub(super) fn next_round(&mut self) -> bool {
        self.rounds = self.rounds.saturating_add(1);
        self.rounds <= MAX_SOURCE_REFINEMENT_ROUNDS
    }

    pub(super) fn exhausted(&self) -> bool {
        self.rounds > MAX_SOURCE_REFINEMENT_ROUNDS
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// A fact location qualified by its owning project module.
pub(super) struct QualifiedEvent {
    pub(super) module: ModuleId,
    pub(super) fact: FactId,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Monotone flow state carried through one qualified call context.
pub(super) struct CrossFlowState {
    pub(super) flow: FlowId,
    /// The source witness carried by this context. `None` represents a
    /// reaching call-site alternative for which this flow has no complete
    /// source proof. Keeping that alternative is what lets cross-call
    /// certainty distinguish `Possible` from `Definite` without inventing a
    /// source from another call site.
    pub(super) source: Option<QualifiedEvent>,
    pub(super) requirements: RequirementSet<QualifiedEvent>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Worklist context identifying the function/value path currently projected.
pub(super) struct CallContext {
    pub(super) module: ModuleId,
    pub(super) function: FunctionId,
    pub(super) parameter: Option<usize>,
    pub(super) source_root: Option<ValueId>,
    pub(super) state: CrossFlowState,
    pub(super) crossed: bool,
}
