#[cfg(test)]
use crate::api::classification::RuleIndex;
use crate::{
    analysis::{
        facts::FactId,
        flow::effect::{EffectArgument, EffectUse, FunctionEffect, ParameterRef},
        model::{
            flow::{FlowId, FlowReadiness, LifecycleEvidence, RequirementIndex, SinkIndex},
            scope::FunctionId,
            value::ValueId,
        },
        trace::QualifiedEvent,
    },
    project::ModuleId,
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EvidenceTransition {
    Advanced,
    AlreadyRecorded,
    Ready,
}

impl EvidenceTransition {
    pub(super) fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub(super) fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Ready, _) | (_, Self::Ready) => Self::Ready,
            (Self::Advanced, _) | (_, Self::Advanced) => Self::Advanced,
            _ => Self::AlreadyRecorded,
        }
    }
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

    pub(super) fn advance_requirement(
        &mut self,
        index: RequirementIndex,
        event: QualifiedEvent,
        readiness: FlowReadiness,
    ) -> EvidenceTransition {
        let recorded = self.evidence.record_requirement(index, event);
        self.classify_requirement(recorded, readiness)
    }

    pub(super) fn advance_sink(
        &mut self,
        index: SinkIndex,
        event: QualifiedEvent,
        readiness: FlowReadiness,
    ) -> EvidenceTransition {
        let recorded = self.evidence.record_sink(index, event);
        self.classify_sink(recorded, readiness)
    }

    pub(super) fn requirement_transition(&self, readiness: FlowReadiness) -> EvidenceTransition {
        self.classify_requirement(false, readiness)
    }

    fn classify_requirement(&self, recorded: bool, readiness: FlowReadiness) -> EvidenceTransition {
        if self.evidence.requirements_ready(readiness) {
            EvidenceTransition::Ready
        } else if recorded {
            EvidenceTransition::Advanced
        } else {
            EvidenceTransition::AlreadyRecorded
        }
    }

    pub(super) fn sink_transition(&self, readiness: FlowReadiness) -> EvidenceTransition {
        self.classify_sink(false, readiness)
    }

    #[cfg(test)]
    pub(super) fn record_requirement_for_test(
        &mut self,
        index: RequirementIndex,
        event: QualifiedEvent,
    ) {
        self.evidence.record_requirement(index, event);
    }

    #[cfg(test)]
    pub(super) fn record_sink_for_test(&mut self, index: SinkIndex, event: QualifiedEvent) {
        self.evidence.record_sink(index, event);
    }

    fn classify_sink(&self, recorded: bool, readiness: FlowReadiness) -> EvidenceTransition {
        if self.source.is_some() && self.evidence.complete(readiness) {
            EvidenceTransition::Ready
        } else if recorded {
            EvidenceTransition::Advanced
        } else {
            EvidenceTransition::AlreadyRecorded
        }
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
enum CallContextOrigin {
    SourceRoot(ValueId),
    TargetParameter(usize),
    #[cfg(test)]
    Unknown,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Worklist context identifying the function/value path currently projected.
pub(super) struct CallContext {
    module: ModuleId,
    function: FunctionId,
    origin: CallContextOrigin,
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
            origin: CallContextOrigin::SourceRoot(source_root),
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
            origin: CallContextOrigin::TargetParameter(parameter),
            state,
            crossed,
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(module: ModuleId, function: FunctionId, state: CrossFlowState) -> Self {
        Self {
            module,
            function,
            origin: CallContextOrigin::Unknown,
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

    fn matches_parameter(
        &self,
        parameter: usize,
        parameter_is_root: bool,
        argument_is_root: bool,
    ) -> bool {
        matches!(self.origin, CallContextOrigin::TargetParameter(candidate) if candidate == parameter)
            && parameter_is_root
            && argument_is_root
    }

    fn matches_source_root(
        &self,
        value: ValueId,
        value_is_root: bool,
        require_value_root: bool,
    ) -> bool {
        matches!(self.origin, CallContextOrigin::SourceRoot(root) if root == value)
            && (!require_value_root || value_is_root)
    }

    pub(super) fn matches_argument(
        &self,
        effect: &FunctionEffect,
        argument: &EffectArgument,
    ) -> bool {
        argument
            .parameter()
            .is_some_and(|parameter| self.matches_parameter_ref(parameter, argument.is_root()))
            || self.matches_source_value(effect, argument.value(), argument.is_root(), true)
    }

    pub(super) fn matches_call_receiver(&self, receiver: &ParameterRef) -> bool {
        self.matches_parameter_ref(receiver, true)
    }

    pub(super) fn matches_property_write(
        &self,
        effect: &FunctionEffect,
        receiver: Option<&ParameterRef>,
        receiver_value: ValueId,
    ) -> bool {
        receiver.is_some_and(|parameter| self.matches_parameter_ref(parameter, true))
            || self.matches_source_value(effect, receiver_value, false, false)
    }

    pub(super) fn matches_use(&self, effect: &FunctionEffect, usage: &EffectUse) -> bool {
        match usage {
            EffectUse::PropertyWrite {
                receiver,
                receiver_value,
                ..
            } => self.matches_property_write(effect, receiver.as_ref(), *receiver_value),
            EffectUse::CallReceiver { receiver, .. } => self.matches_call_receiver(receiver),
            EffectUse::CallArgument {
                call_id,
                argument_index,
                ..
            } => effect
                .call_argument(*call_id, *argument_index)
                .is_some_and(|argument| self.matches_argument(effect, argument)),
        }
    }

    fn matches_parameter_ref(&self, parameter: &ParameterRef, argument_is_root: bool) -> bool {
        self.matches_parameter(parameter.index(), parameter.is_root(), argument_is_root)
    }

    fn matches_source_value(
        &self,
        effect: &FunctionEffect,
        value: ValueId,
        value_is_root: bool,
        require_value_root: bool,
    ) -> bool {
        self.matches_source_root(effect.root_value(value), value_is_root, require_value_root)
    }
}

#[cfg(test)]
mod tests;
