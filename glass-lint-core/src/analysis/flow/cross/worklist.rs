use std::collections::BTreeSet;

use glass_lint_datastructures::BoundedFifo;
pub(super) use glass_lint_datastructures::FifoAdmission;

use crate::{
    analysis::{
        ProjectSemanticModel, QualifiedFunctionId,
        flow::cross::{
            MAX_CONTEXTS,
            graph::QualifiedCallGraph,
            sources::{FlowSources, SourceKey},
            state::{CallContext, CrossFlowState},
        },
        model::{flow::FlowId, scope::FunctionId},
        trace::QualifiedEvent,
    },
    project::ModuleId,
};

/// Deduplicating FIFO worklist for bounded interprocedural contexts.
///
/// Uses `VecDeque` for O(1) pop-front and a `BTreeSet` for O(log n) dedup,
/// avoiding the O(n) shift cost of `IndexSet::shift_remove_index(0)`.
///
/// The worklist enforces [`MAX_CONTEXTS`] total retained contexts so that
/// the seen-set (not only the pending frontier) is bounded.
pub(super) struct ContextWorklist {
    fifo: BoundedFifo<CallContext>,
}

impl ContextWorklist {
    pub(super) fn new(max_retained: usize) -> Self {
        Self {
            fifo: BoundedFifo::new(max_retained),
        }
    }

    /// Push a context if the total-retained budget allows.
    ///
    /// Admit one context, distinguishing deduplication from a rejected new
    /// context at the retained bound.
    pub(super) fn push(&mut self, context: CallContext) -> FifoAdmission {
        self.fifo.push(context)
    }

    pub(super) fn pop_front(&mut self) -> Option<CallContext> {
        self.fifo.pop_front()
    }

    /// Total unique contexts retained (seen-set size).
    ///
    /// This is the meaningful bound for worklist memory: it counts every
    /// unique context ever inserted, not only the pending frontier.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.fifo.retained_len()
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.fifo.is_exhausted()
    }

    pub(super) fn enqueue_parameters(
        &mut self,
        project: &ProjectSemanticModel,
        module: ModuleId,
        function: FunctionId,
        argument_index: usize,
        state: &CrossFlowState,
        crossed: bool,
    ) {
        let Some(effect) = project.effect(QualifiedFunctionId::new(module, function)) else {
            return;
        };
        let Some(fact_stream) = project.module_fact_stream(module) else {
            return;
        };
        let Some(parameters) = effect.parameters(fact_stream) else {
            return;
        };
        for parameter in parameters
            .iter()
            .filter(|parameter| parameter.is_root_for(argument_index))
        {
            if self.is_exhausted() {
                return;
            }
            self.push(CallContext::for_target_call(
                module,
                function,
                parameter.parameter_index(),
                state.clone(),
                crossed,
            ));
        }
    }

    pub(super) fn seed(
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
    ) -> Self {
        let mut worklist = Self::new(MAX_CONTEXTS);
        worklist.seed_from_sources(project, sources);
        let source_flows = sources
            .propagation_entries()
            .map(|(_, candidate)| candidate.flow_id())
            .collect::<BTreeSet<_>>();
        worklist.seed_from_calls(project, sources, call_graph, &source_flows);
        worklist
    }

    fn seed_from_sources(&mut self, project: &ProjectSemanticModel, sources: &FlowSources) {
        for (key, candidate) in sources.propagation_entries() {
            if self.is_exhausted() {
                return;
            }
            self.push(CallContext::for_source(
                key.module(),
                key.function(),
                key.value(),
                CrossFlowState::known(
                    candidate.flow_id(),
                    QualifiedEvent::new(key.module(), candidate.event()),
                ),
                key.value()
                    != project
                        .source_call_result(QualifiedEvent::new(key.module(), candidate.event())),
            ));
        }
    }

    fn seed_from_calls(
        &mut self,
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
        source_flows: &BTreeSet<FlowId>,
    ) {
        for module in project.modules() {
            if self.is_exhausted() {
                return;
            }
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let Some(target) =
                        call_graph.get(QualifiedEvent::new(module.id(), call.event()))
                    else {
                        continue;
                    };
                    for argument in call.arguments() {
                        if !argument.is_root() {
                            continue;
                        }
                        let root = effect.root_value(argument.value());
                        let source_key = SourceKey::new(module.id(), effect.id(), root);
                        let candidates: Vec<_> = sources.candidates(&source_key).copied().collect();
                        let present = candidates
                            .iter()
                            .map(|candidate| candidate.flow_id())
                            .collect::<BTreeSet<_>>();
                        for candidate in candidates {
                            let state = CrossFlowState::known(
                                candidate.flow_id(),
                                QualifiedEvent::new(module.id(), candidate.event()),
                            );
                            self.enqueue_parameters(
                                project,
                                target.module(),
                                target.function(),
                                argument.index(),
                                &state,
                                target.module() != module.id(),
                            );
                        }

                        // A call-site without a source candidate is still a
                        // modeled reaching alternative. Carry it through the
                        // same parameter/call projection with an explicit
                        // unknown source so it can downgrade a matching
                        // witness to Possible without contributing evidence.
                        for &flow in source_flows.difference(&present) {
                            self.enqueue_parameters(
                                project,
                                target.module(),
                                target.function(),
                                argument.index(),
                                &CrossFlowState::unknown(flow),
                                target.module() != module.id(),
                            );
                        }
                    }
                }
            }
        }
    }
}
