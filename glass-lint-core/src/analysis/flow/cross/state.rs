#[cfg(test)]
use glass_lint_datastructures::Budget;

use crate::{
    analysis::{
        facts::FactId,
        model::flow::{FlowId, IndexedEvidence, RequirementIndex, SinkIndex},
        value::{FunctionId, ValueId},
    },
    api::compiler::{CompiledObjectFlow, object_flow::CompletionMode},
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
    flow: FlowId,
    /// The source witness carried by this context. `None` represents a
    /// reaching call-site alternative for which this flow has no complete
    /// source proof. Keeping that alternative is what lets cross-call
    /// certainty distinguish `Possible` from `Definite` without inventing a
    /// source from another call site.
    source: Option<QualifiedEvent>,
    requirements: IndexedEvidence<QualifiedEvent, RequirementIndex>,
    sinks: IndexedEvidence<QualifiedEvent, SinkIndex>,
}

impl CrossFlowState {
    pub(super) fn known(flow: FlowId, source: QualifiedEvent) -> Self {
        Self {
            flow,
            source: Some(source),
            requirements: IndexedEvidence::default(),
            sinks: IndexedEvidence::default(),
        }
    }

    pub(super) fn unknown(flow: FlowId) -> Self {
        Self {
            flow,
            source: None,
            requirements: IndexedEvidence::default(),
            sinks: IndexedEvidence::default(),
        }
    }

    pub(super) fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub(super) fn source(&self) -> Option<&QualifiedEvent> {
        self.source.as_ref()
    }

    pub(super) fn record_requirement(
        &mut self,
        index: RequirementIndex,
        event: QualifiedEvent,
    ) -> bool {
        self.requirements.insert(index, event)
    }

    pub(super) fn record_sink(&mut self, index: SinkIndex, event: QualifiedEvent) -> bool {
        self.sinks.insert(index, event)
    }

    pub(super) fn requirements_ready(&self, flow: &CompiledObjectFlow) -> bool {
        flow.requirements_ready(self.requirements.len())
    }

    pub(super) fn sinks_complete(&self, flow: &CompiledObjectFlow) -> bool {
        flow.completion_mode != CompletionMode::AllSinks || self.sinks.len() == flow.sinks.len()
    }

    pub(super) fn requirement_events(&self) -> impl Iterator<Item = &QualifiedEvent> {
        self.requirements.values()
    }

    pub(super) fn prior_sinks(&self, module: ModuleId, event: FactId) -> Vec<QualifiedEvent> {
        let mut sinks: Vec<_> = self
            .sinks
            .values()
            .filter(|sink| !(sink.module == module && sink.fact == event))
            .cloned()
            .collect();
        sinks.sort();
        sinks.dedup();
        sinks
    }
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
