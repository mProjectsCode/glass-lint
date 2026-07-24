use crate::analysis::facts::FactId;
use crate::analysis::flow::index::FlowId;
use crate::analysis::flow::requirements::RequirementSet;
use crate::analysis::value::{FunctionId, ValueId};
use crate::project::ModuleId;

use super::MAX_SOURCE_REFINEMENT_ROUNDS;

#[derive(Clone, Copy)]
pub(super) enum EvidenceRole {
    Source,
    Requirement,
    Sink,
}

impl EvidenceRole {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Source => "flow source",
            Self::Requirement => "flow requirement",
            Self::Sink => "flow sink",
        }
    }
}

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
    pub(super) source: QualifiedEvent,
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
