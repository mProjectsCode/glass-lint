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
            sink::{FunctionSignature, FunctionSinkSummary, FunctionSummary},
        },
    },
    model::{flow::FunctionTable, scope::FunctionId, value::ValueId},
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
        for _ in 0..inserted {
            if !budget.try_push() {
                return Err(FlowCompletion::incomplete(
                    FlowCompletionReason::SummaryBudget,
                ));
            }
        }
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
        self.retained = retained;
        Ok(())
    }
}

impl FlowCompletion {
    fn finalize(summaries: &mut FunctionSummaries<'_>) {
        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sort_sinks();
        }
    }
}

impl<'a> FunctionSummaries<'a> {
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
            summaries.completion =
                SummaryPropagation::new(stream, &summaries.by_id).run(&mut summaries, budget);
        }
        FlowCompletion::finalize(&mut summaries);
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
        let entries: Vec<(FunctionId, usize)> = self
            .by_id
            .iter()
            .map(|(id, summary)| (id, summary.calls().len()))
            .collect();
        for (id, count) in entries {
            if self.completion.is_incomplete() {
                return;
            }
            let Some(summary) = self.by_id.get_mut(id) else {
                continue;
            };
            for idx in 0..count {
                if self.completion.is_incomplete() {
                    return;
                }
                if let Some(call_id) = summary.calls().get(idx).copied() {
                    let added = summary
                        .collect_sinks_for_call(stream, plan, &mut self.paths, call_id)
                        .inserted();
                    if let Err(completion) = self.sink_budget.admit(budget, added) {
                        self.completion = completion;
                        return;
                    }
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
        let mut inserted_any = false;
        for proj in projections {
            let inserted = caller_summary.add_sink(proj).inserted();
            inserted_any |= inserted > 0;
            self.sink_budget.admit(budget, inserted)?;
        }
        Ok(inserted_any)
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
                let call_count = summaries
                    .by_id
                    .get(caller)
                    .map_or(0, |summary| summary.calls().len());
                for index in 0..call_count {
                    let Some(call_id) = summaries
                        .by_id
                        .get(caller)
                        .and_then(|summary| summary.calls().get(index))
                        .copied()
                    else {
                        continue;
                    };
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
    let FactPayload::Call {
        target_function,
        args,
        ..
    } = &stream.fact(call_id)?.payload
    else {
        return None;
    };
    Some(((*target_function)?, args))
}

fn try_project_sink(
    target_parameters: &[ParameterBinding],
    caller_parameters: &[ParameterBinding],
    sink: &FunctionSinkSummary,
    stream: &FactStream<Frozen>,
    args: &[CallArgInfo],
    paths: &SummaryPathStore<'_>,
) -> Option<FunctionSinkSummary> {
    let target_parameter = target_parameters.iter().find(|parameter| {
        parameter.parameter_index() == sink.parameter_index()
            && parameter.matches_sink_path(sink.path(), paths)
    })?;
    let argument = target_parameter.project_argument_at(stream, args, paths, sink.path())?;
    let caller_parameter = caller_parameters.iter().find(|parameter| {
        !parameter.is_rest()
            && parameter.value() != ValueId::UNKNOWN
            && parameter.value() == argument
    })?;
    let caller_path = paths.intern_frozen(caller_parameter.path())?;
    Some(FunctionSinkSummary::new(
        sink.flow(),
        caller_parameter.parameter_index(),
        caller_path,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        facts,
        flow::{effect::FunctionEffects, planning::BoundFlowPlan},
    };

    fn unlimited_budget() -> Budget {
        Budget::new(usize::MAX)
    }

    #[test]
    fn same_name_siblings_are_keyed_by_function_id() {
        let source = "function first(x) { document.body.appendChild(x); } function second(x) { console.log(x); }";
        let stream = facts::build_test_facts(source, "summary-siblings.js");
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let mut budget = unlimited_budget();
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan, &mut budget);
        assert!(summaries.by_id.len() >= 2);
        assert_eq!(
            summaries
                .by_id
                .iter()
                .map(|(_, summary)| summary)
                .filter(|summary| summary.parameter_count() == 1)
                .count(),
            2
        );
    }

    #[test]
    fn sink_propagates_from_callee_to_caller_through_parameter() {
        let source = "\
            function sink(x) { document.body.appendChild(x); }\
            function bridge(y) { sink(y); }\
        ";
        let stream = facts::build_test_facts(source, "sink-propagation.js");
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let mut budget = unlimited_budget();
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan, &mut budget);
        let bridge = summaries
            .get(FunctionId::from_test(2))
            .expect("bridge function should have a summary");
        assert!(
            bridge.parameter_count() >= 1,
            "bridge has at least one parameter"
        );
    }

    #[test]
    fn collect_skips_invalid_function_effects() {
        let source = "\
            function a() { return 1; }\
            function b(x) { return x; }\
        ";
        let stream = facts::build_test_facts(source, "multiple-functions.js");
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let mut budget = unlimited_budget();
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan, &mut budget);
        assert!(
            summaries.get(FunctionId::from_test(1)).is_none(),
            "a returns a constant and should be filtered as invalid"
        );
        assert!(
            summaries.get(FunctionId::from_test(2)).is_some(),
            "b returns a parameter and should have a summary"
        );
    }

    #[test]
    fn invoke_compatible_rejects_too_many_args() {
        let source = "function f(a) {} f(1, 2, 3);";
        let stream = facts::build_test_facts(source, "too-many-args.js");
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let mut budget = unlimited_budget();
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan, &mut budget);
        let f = summaries
            .get(FunctionId::from_test(1))
            .expect("f should have a summary");
        let _callee_params = stream.function_parameters(FunctionId::from_test(1));
        let call_fact = stream
            .facts()
            .iter()
            .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .expect("call fact should exist");
        let FactPayload::Call { args, .. } = &call_fact.payload else {
            unreachable!()
        };
        assert!(!f.is_invocation_compatible(&stream, args, &summaries.paths));
    }

    #[test]
    fn invoke_compatible_rejects_spread_args() {
        let source = "function f(a) {} f(...args);";
        let stream = facts::build_test_facts(source, "spread-args.js");
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let mut budget = unlimited_budget();
        let summaries = FunctionSummaries::collect(&stream, &effects, &plan, &mut budget);
        let f = summaries
            .get(FunctionId::from_test(1))
            .expect("f should have a summary");
        let call_fact = stream
            .facts()
            .iter()
            .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .expect("call fact should exist");
        let FactPayload::Call { args, .. } = &call_fact.payload else {
            unreachable!()
        };
        assert!(!f.is_invocation_compatible(&stream, args, &summaries.paths));
    }
}
