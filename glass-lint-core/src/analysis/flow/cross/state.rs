#[cfg(test)]
use glass_lint_datastructures::Budget;

use crate::{
    analysis::{
        facts::FactId,
        model::flow::{FlowId, RequirementSet},
        value::{FunctionId, ValueId},
    },
    project::ModuleId,
};

#[derive(Debug)]
/// Per-transfer budget for propagating source identities through helper calls.
///
/// Charges each candidate insertion as one operation so that a long or
/// cyclical propagation graph is bounded by work done, not by an arbitrary
/// round count.
#[cfg(test)]
pub(super) struct SourceBudget {
    inner: Budget,
}

#[cfg(test)]
impl SourceBudget {
    pub(super) fn new(operations: usize) -> Self {
        Self {
            inner: Budget::new(operations),
        }
    }

    /// Charge for one candidate transfer. Returns `false` when exhausted.
    pub(super) fn try_charge(&mut self) -> bool {
        self.inner.try_push()
    }

    pub(super) fn exhausted(&self) -> bool {
        self.inner.exhausted()
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
    pub(super) sinks: RequirementSet<QualifiedEvent>,
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
