use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::Budget;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen, ParameterBinding},
    flow::{
        FlowCompletion, FlowCompletionReason,
        effect::{EffectCall, FunctionEffects},
        planning::BoundFlowPlan,
        summary::{
            MAX_SUMMARY_SINKS, MAX_SUMMARY_WORKLIST, SummaryPathStore,
            sink::{
                FunctionSignature, FunctionSinkSummary, FunctionSummary, find_sink_parameter,
                parameter_for_value,
            },
        },
    },
    model::{flow::FunctionTable, scope::FunctionId},
};

#[derive(Debug)]
pub(in crate::analysis::flow) struct FunctionSummaries<'a> {
    stream: &'a FactStream<Frozen>,
    by_id: FunctionTable<FunctionSummary>,
    paths: SummaryPathStore<'a>,
    completion: FlowCompletion,
    sink_budget: SummarySinkBudget,
}

#[derive(Debug, Default)]
struct SummarySinkBudget {
    retained: usize,
}

impl SummarySinkBudget {
    fn admit(&mut self, budget: &mut Budget, inserted: usize) -> Result<(), FlowCompletion> {
        let Some(retained) = self.retained.checked_add(inserted) else {
            return Err(FlowCompletion::incomplete(
                FlowCompletionReason::SummarySinkCapacity,
            ));
        };
        if retained > MAX_SUMMARY_SINKS {
            return Err(FlowCompletion::incomplete(
                FlowCompletionReason::SummarySinkCapacity,
            ));
        }
        for _ in 0..inserted {
            if !budget.try_push() {
                return Err(FlowCompletion::incomplete(
                    FlowCompletionReason::SummaryBudget,
                ));
            }
        }
        self.retained = retained;
        Ok(())
    }

    fn admit_sinks(
        &mut self,
        summary: &mut FunctionSummary,
        candidates: Vec<FunctionSinkSummary>,
        budget: &mut Budget,
    ) -> Result<bool, FlowCompletion> {
        let new_count = summary.new_sink_count(&candidates);
        self.admit(budget, new_count)?;
        if new_count == 0 {
            return Ok(false);
        }
        summary.add_sinks(candidates);
        Ok(true)
    }
}

impl<'a> FunctionSummaries<'a> {
    fn finalize(&mut self) {
        for (_, summary) in self.by_id.iter_mut() {
            summary.sort_sinks();
        }
    }

    pub(in crate::analysis::flow) fn completion(&self) -> FlowCompletion {
        self.completion
    }

    pub(in crate::analysis::flow) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(id)
    }

    pub(in crate::analysis::flow) fn path_interner(&self) -> &SummaryPathStore<'a> {
        &self.paths
    }

    fn insert(&mut self, summary: FunctionSummary) {
        let _ = self.by_id.insert(summary.id(), summary);
    }

    pub(in crate::analysis::flow) fn collect(
        stream: &'a FactStream<Frozen>,
        effects: &FunctionEffects,
        plan: &BoundFlowPlan<'_>,
        budget: &mut Budget,
    ) -> Self {
        let mut summaries = Self {
            stream,
            by_id: FunctionTable::new(stream.function_count()),
            paths: SummaryPathStore::new(stream.paths()),
            completion: FlowCompletion::default(),
            sink_budget: SummarySinkBudget::default(),
        };
        summaries.collect_facts(effects, budget);
        if summaries.completion.is_complete() {
            summaries.collect_direct_sinks(stream, plan, budget);
        }
        if summaries.completion.is_complete() {
            let mut propagation = SummaryPropagation::new(stream, &summaries.by_id);
            let completion = propagation.run(&mut summaries, budget);
            summaries.completion.merge(completion);
        }
        summaries.finalize();
        summaries
    }

    fn exhaust(&mut self, reason: FlowCompletionReason) {
        self.completion.mark(reason);
    }

    fn collect_facts(&mut self, effects: &FunctionEffects, budget: &mut Budget) {
        for effect in effects.iter_effects() {
            if effect.is_invalid() {
                continue;
            }
            if self.get(effect.id()).is_none() {
                if !budget.try_push() {
                    self.exhaust(FlowCompletionReason::SummaryBudget);
                    return;
                }
                let Some(params) = effect.parameters(self.stream) else {
                    self.exhaust(FlowCompletionReason::Summary);
                    continue;
                };
                self.insert(FunctionSummary::new(
                    effect.id(),
                    FunctionSignature::from_bindings(params),
                    effect.calls().iter().map(EffectCall::event).collect(),
                ));
            }
        }
    }

    fn collect_direct_sinks(
        &mut self,
        stream: &FactStream<Frozen>,
        plan: &BoundFlowPlan<'_>,
        budget: &mut Budget,
    ) {
        let entries: Vec<(FunctionId, Vec<FactId>)> = self
            .by_id
            .iter()
            .map(|(id, summary)| (id, summary.call_ids()))
            .collect();
        for (id, call_ids) in entries {
            if self.completion.is_incomplete() {
                return;
            }
            let Some(summary) = self.by_id.get_mut(id) else {
                continue;
            };
            for call_id in call_ids {
                if self.completion.is_incomplete() {
                    return;
                }
                let candidates =
                    summary.collect_sinks_for_call(stream, plan, &mut self.paths, call_id);
                if let Err(completion) = self.sink_budget.admit_sinks(summary, candidates, budget) {
                    self.completion = completion;
                    return;
                }
            }
        }
    }

    fn propagate_call_sinks(
        &mut self,
        call_id: FactId,
        caller: FunctionId,
        stream: &FactStream<Frozen>,
        budget: &mut Budget,
    ) -> Result<bool, FlowCompletion> {
        let Some((target, args)) = resolve_call_target(call_id, stream) else {
            return Ok(false);
        };
        if target == caller {
            return Ok(false);
        }
        let Some((target_summary, caller_summary)) = self.by_id.get_disjoint(target, caller) else {
            return Ok(false);
        };
        let target_summary = match target_summary {
            Some(s) if s.is_invocation_compatible(stream, args, &self.paths) => s,
            _ => return Ok(false),
        };
        let Some(caller_summary) = caller_summary else {
            return Ok(false);
        };
        let mut projections = Vec::new();
        {
            let Some(target_params) = stream.function_parameters(target) else {
                return Ok(false);
            };
            let Some(caller_params) = stream.function_parameters(caller) else {
                return Ok(false);
            };
            for sink in target_summary.sinks() {
                if let Some(proj) = try_project_sink(
                    target_params,
                    caller_params,
                    sink,
                    stream,
                    args,
                    &self.paths,
                ) {
                    projections.push(proj);
                }
            }
        }
        self.sink_budget
            .admit_sinks(caller_summary, projections, budget)
    }
}

struct SummaryPropagation<'a> {
    stream: &'a FactStream<Frozen>,
    reverse_calls: BTreeMap<FunctionId, Vec<FunctionId>>,
    worklist: BTreeSet<FunctionId>,
}

impl<'a> SummaryPropagation<'a> {
    fn new(stream: &'a FactStream<Frozen>, summaries: &FunctionTable<FunctionSummary>) -> Self {
        let mut reverse_calls: BTreeMap<FunctionId, Vec<FunctionId>> = BTreeMap::new();
        for (caller_id, summary) in summaries.iter() {
            for call_id in summary.calls() {
                if let Some((target, _)) = resolve_call_target(*call_id, stream)
                    && target != caller_id
                {
                    reverse_calls.entry(target).or_default().push(caller_id);
                }
            }
        }
        for callers in reverse_calls.values_mut() {
            callers.sort_unstable();
            callers.dedup();
        }
        Self {
            stream,
            reverse_calls,
            worklist: summaries.iter().map(|(id, _)| id).collect(),
        }
    }

    fn run(
        &mut self,
        summaries: &mut FunctionSummaries<'a>,
        budget: &mut Budget,
    ) -> FlowCompletion {
        while !self.worklist.is_empty() {
            if !budget.try_push() {
                return FlowCompletion::incomplete(FlowCompletionReason::SummaryBudget);
            }
            let current_round: Vec<FunctionId> = self.worklist.iter().copied().collect();
            self.worklist.clear();
            let mut changed = BTreeSet::new();
            for caller in current_round {
                let call_ids = summaries
                    .by_id
                    .get(caller)
                    .map_or(Vec::new(), FunctionSummary::call_ids);
                for call_id in call_ids {
                    let changed_now = match summaries.propagate_call_sinks(
                        call_id,
                        caller,
                        self.stream,
                        budget,
                    ) {
                        Ok(changed) => changed,
                        Err(outcome) => return outcome,
                    };
                    if changed_now {
                        changed.insert(caller);
                    }
                }
            }
            for changed_id in changed {
                if let Some(callers) = self.reverse_calls.get(&changed_id) {
                    for &caller in callers {
                        if self.worklist.len() >= MAX_SUMMARY_WORKLIST {
                            return FlowCompletion::incomplete(
                                FlowCompletionReason::SummaryWorklistCapacity,
                            );
                        }
                        self.worklist.insert(caller);
                    }
                }
            }
        }
        FlowCompletion::default()
    }
}

fn resolve_call_target(
    call_id: FactId,
    stream: &FactStream<Frozen>,
) -> Option<(FunctionId, &[CallArgInfo])> {
    let FactPayload::Call(call) = stream.fact(call_id)?.payload() else {
        return None;
    };
    Some((call.target_function()?, call.args()))
}

fn try_project_sink(
    target_parameters: &[ParameterBinding],
    caller_parameters: &[ParameterBinding],
    sink: &FunctionSinkSummary,
    stream: &FactStream<Frozen>,
    args: &[CallArgInfo],
    paths: &SummaryPathStore<'_>,
) -> Option<FunctionSinkSummary> {
    let target_parameter = find_sink_parameter(target_parameters, sink, paths)?;
    let argument = target_parameter.project_argument_at(stream, args, paths, sink.path())?;
    let caller_parameter = parameter_for_value(caller_parameters, argument)
        .filter(|parameter| !parameter.is_rest())?;
    let caller_path = paths.intern_frozen(caller_parameter.path())?;
    Some(FunctionSinkSummary::new(
        sink.flow(),
        caller_parameter.parameter_index(),
        caller_path,
    ))
}

#[cfg(test)]
mod tests;
