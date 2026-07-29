use std::collections::{BTreeSet, VecDeque};

use crate::{
    analysis::{
        ProjectSemanticModel,
        flow::cross::{
            MAX_CONTEXTS,
            graph::QualifiedCallGraph,
            sources::{FlowSources, SourceKey},
            state::{CallContext, CrossFlowState, QualifiedEvent},
        },
        model::flow::RequirementSet,
        value::FunctionId,
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
    /// FIFO queue of pending contexts.
    queue: VecDeque<CallContext>,
    /// Seen-set for O(log n) deduplication and total-retained tracking.
    seen: BTreeSet<CallContext>,
    /// Maximum unique contexts retained before dropping new ones.
    max_retained: usize,
}

impl ContextWorklist {
    pub(super) fn new(max_retained: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            seen: BTreeSet::new(),
            max_retained,
        }
    }

    /// Push a context if the total-retained budget allows.
    ///
    /// Returns whether the context was newly added.  Contexts beyond the
    /// retained limit are silently dropped (the caller will detect
    /// exhaustion via [`len`] / [`is_exhausted`]).
    pub(super) fn push(&mut self, context: CallContext) -> bool {
        if self.seen.len() >= self.max_retained {
            return false;
        }
        if self.seen.insert(context.clone()) {
            self.queue.push_back(context);
            true
        } else {
            false
        }
    }

    pub(super) fn pop_front(&mut self) -> Option<CallContext> {
        let context = self.queue.pop_front()?;
        Some(context)
    }

    /// Total unique contexts retained (seen-set size).
    ///
    /// This is the meaningful bound for worklist memory: it counts every
    /// unique context ever inserted, not only the pending frontier.
    pub(super) fn len(&self) -> usize {
        self.seen.len()
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.seen.len() >= self.max_retained
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
        let Some(effect) = project.effect(module, function) else {
            return;
        };
        let Some(fact_stream) = project.module_fact_stream(module) else {
            return;
        };
        for parameter in effect.parameters(fact_stream).iter().filter(|parameter| {
            parameter.parameter_index == argument_index && parameter.path.is_empty()
        }) {
            if self.is_exhausted() {
                return;
            }
            self.push(CallContext {
                module,
                function,
                parameter: Some(parameter.parameter_index),
                source_root: None,
                state: state.clone(),
                crossed,
            });
        }
    }

    pub(super) fn seed(
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
        flows: &[crate::analysis::model::flow::FlowId],
    ) -> Self {
        let mut worklist = Self::new(MAX_CONTEXTS);
        worklist.seed_from_sources(project, sources);
        worklist.seed_from_calls(project, sources, call_graph, flows);
        worklist
    }

    fn seed_from_sources(&mut self, project: &ProjectSemanticModel, sources: &FlowSources) {
        for (key, candidates) in sources.iter() {
            if self.is_exhausted() {
                return;
            }
            for candidate in candidates {
                self.push(CallContext {
                    module: key.module,
                    function: key.function,
                    parameter: None,
                    source_root: Some(key.value),
                    state: CrossFlowState {
                        flow: candidate.flow,
                        source: Some(QualifiedEvent {
                            module: key.module,
                            fact: candidate.fact,
                        }),
                        requirements: RequirementSet::default(),
                        sinks: RequirementSet::default(),
                    },
                    crossed: key.value != project.source_call_result(key.module, candidate.fact),
                });
            }
        }
    }

    fn seed_from_calls(
        &mut self,
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
        flows: &[crate::analysis::model::flow::FlowId],
    ) {
        for module in project.modules() {
            if self.is_exhausted() {
                return;
            }
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let Some((target_module, target_function)) =
                        call_graph.get(module.id(), call.event())
                    else {
                        continue;
                    };
                    for argument in call.arguments() {
                        if !argument.is_root() {
                            continue;
                        }
                        let root = effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value());
                        let source_key = SourceKey::new(module.id(), effect.id(), root);
                        if let Some(candidates) = sources.get(&source_key) {
                            for candidate in candidates {
                                let state = CrossFlowState {
                                    flow: candidate.flow,
                                    source: Some(QualifiedEvent {
                                        module: module.id(),
                                        fact: candidate.fact,
                                    }),
                                    requirements: RequirementSet::default(),
                                    sinks: RequirementSet::default(),
                                };
                                self.enqueue_parameters(
                                    project,
                                    target_module,
                                    target_function,
                                    argument.index(),
                                    &state,
                                    target_module != module.id(),
                                );
                            }
                        }

                        // A call-site without a source candidate is still a
                        // modeled reaching alternative. Carry it through the
                        // same parameter/call projection with an explicit
                        // unknown source so it can downgrade a matching
                        // witness to Possible without contributing evidence.
                        for &flow in flows {
                            let has_source = sources
                                .get(&source_key)
                                .is_some_and(|items| items.iter().any(|item| item.flow == flow));
                            if has_source {
                                continue;
                            }
                            self.enqueue_parameters(
                                project,
                                target_module,
                                target_function,
                                argument.index(),
                                &CrossFlowState {
                                    flow,
                                    source: None,
                                    requirements: RequirementSet::default(),
                                    sinks: RequirementSet::default(),
                                },
                                target_module != module.id(),
                            );
                        }
                    }
                }
            }
        }
    }
}
