#[cfg(test)]
use glass_lint_datastructures::Budget;

pub(super) use crate::analysis::trace::QualifiedEvent;
use crate::{
    analysis::{
        facts::FactId,
        model::flow::{FlowId, LifecycleEvidence, RequirementIndex, SinkIndex},
        value::{FunctionId, ValueId},
    },
    api::compiler::CompiledObjectFlow,
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
/// Monotone flow state carried through one qualified call context.
pub(super) struct CrossFlowState {
    flow: FlowId,
    /// The source witness carried by this context. `None` represents a
    /// reaching call-site alternative for which this flow has no complete
    /// source proof. Keeping that alternative is what lets cross-call
    /// certainty distinguish `Possible` from `Definite` without inventing a
    /// source from another call site.
    source: Option<QualifiedEvent>,
    evidence: LifecycleEvidence<QualifiedEvent>,
}

impl CrossFlowState {
    pub(super) fn known(flow: FlowId, source: QualifiedEvent) -> Self {
        Self {
            flow,
            source: Some(source),
            evidence: LifecycleEvidence::default(),
        }
    }

    pub(super) fn unknown(flow: FlowId) -> Self {
        Self {
            flow,
            source: None,
            evidence: LifecycleEvidence::default(),
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
        self.evidence.record_requirement(index, event)
    }

    pub(super) fn record_sink(&mut self, index: SinkIndex, event: QualifiedEvent) -> bool {
        self.evidence.record_sink(index, event)
    }

    pub(super) fn requirements_ready(&self, flow: &CompiledObjectFlow) -> bool {
        self.evidence.requirements_ready(flow)
    }

    pub(super) fn sinks_complete(&self, flow: &CompiledObjectFlow) -> bool {
        self.evidence.sinks_ready(flow)
    }

    pub(super) fn requirement_events(&self) -> impl Iterator<Item = &QualifiedEvent> {
        self.evidence.requirement_events()
    }

    pub(super) fn prior_sinks(&self, module: ModuleId, event: FactId) -> Vec<QualifiedEvent> {
        self.evidence
            .prior_sink_events(|sink| sink.module() == module && sink.fact() == event)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Worklist context identifying the function/value path currently projected.
pub(super) struct CallContext {
    module: ModuleId,
    function: FunctionId,
    parameter: Option<usize>,
    source_root: Option<ValueId>,
    state: CrossFlowState,
    crossed: bool,
}

impl CallContext {
    pub(super) fn for_source(
        module: ModuleId,
        function: FunctionId,
        source_root: ValueId,
        state: CrossFlowState,
        crossed: bool,
    ) -> Self {
        Self {
            module,
            function,
            parameter: None,
            source_root: Some(source_root),
            state,
            crossed,
        }
    }

    pub(super) fn for_target_call(
        module: ModuleId,
        function: FunctionId,
        parameter: usize,
        state: CrossFlowState,
        crossed: bool,
    ) -> Self {
        Self {
            module,
            function,
            parameter: Some(parameter),
            source_root: None,
            state,
            crossed,
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(module: ModuleId, function: FunctionId, state: CrossFlowState) -> Self {
        Self {
            module,
            function,
            parameter: None,
            source_root: None,
            state,
            crossed: false,
        }
    }

    pub(super) fn module(&self) -> ModuleId {
        self.module
    }

    pub(super) fn function(&self) -> FunctionId {
        self.function
    }

    pub(super) fn state(&self) -> &CrossFlowState {
        &self.state
    }

    pub(super) fn is_crossed(&self) -> bool {
        self.crossed
    }

    pub(super) fn matches_parameter(
        &self,
        parameter: usize,
        parameter_is_root: bool,
        argument_is_root: bool,
    ) -> bool {
        self.parameter == Some(parameter) && parameter_is_root && argument_is_root
    }

    pub(super) fn matches_source_root(
        &self,
        value: ValueId,
        value_is_root: bool,
        require_value_root: bool,
    ) -> bool {
        self.parameter.is_none()
            && (!require_value_root || value_is_root)
            && self.source_root.is_some_and(|root| root == value)
    }
}
