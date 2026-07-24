use std::collections::{BTreeMap, BTreeSet};

use super::{
    sink::{FunctionSinkSummary, FunctionSummary},
    store::SummaryPathStore,
};
use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen, ParameterBinding},
    flow::{
        effect::{EffectCall, FunctionEffects},
        plan::BoundFlowPlan,
        table::FunctionTable,
    },
    value::{FunctionId, ValueId},
};

#[derive(Debug)]
pub struct FunctionSummaries<'a> {
    stream: &'a FactStream<Frozen>,
    by_id: FunctionTable<FunctionSummary>,
    paths: SummaryPathStore<'a>,
    scratch_projections: Vec<FunctionSinkSummary>,
}

impl<'a> FunctionSummaries<'a> {
    pub(in crate::analysis::flow) fn get(&self, id: FunctionId) -> Option<&FunctionSummary> {
        self.by_id.get(id)
    }

    pub(in crate::analysis::flow) fn path_interner(&self) -> &SummaryPathStore<'a> {
        &self.paths
    }

    fn insert(&mut self, summary: FunctionSummary) {
        self.by_id.insert(summary.id(), summary);
    }

    pub(in crate::analysis::flow) fn collect(
        stream: &'a FactStream<Frozen>,
        effects: &FunctionEffects,
        plan: &BoundFlowPlan<'_>,
    ) -> Self {
        let mut summaries = Self {
            stream,
            by_id: FunctionTable::default(),
            paths: SummaryPathStore::new(stream.paths()),
            scratch_projections: Vec::new(),
        };
        summaries.collect_facts(effects);
        summaries.collect_direct_sinks(stream, plan);
        summaries.propagate_sinks(stream);
        for (_, summary) in summaries.by_id.iter_mut() {
            summary.sort_sinks();
        }
        summaries
    }

    fn collect_facts(&mut self, effects: &FunctionEffects) {
        for effect in effects.iter_effects() {
            if self.get(effect.id()).is_none() {
                let params = effect.parameters(self.stream);
                self.insert(FunctionSummary::new(
                    effect.id(),
                    params
                        .iter()
                        .map(|parameter| parameter.parameter_index)
                        .max()
                        .map_or(0, |index| index.saturating_add(1)),
                    params.iter().any(|parameter| parameter.rest),
                    effect.calls().iter().map(EffectCall::event).collect(),
                ));
            }
        }
    }

    fn collect_direct_sinks(&mut self, stream: &FactStream<Frozen>, plan: &BoundFlowPlan<'_>) {
        let entries: Vec<(FunctionId, usize)> = self
            .by_id
            .iter()
            .map(|(id, summary)| (id, summary.calls().len()))
            .collect();
        for (id, count) in entries {
            let Some(summary) = self.by_id.get_mut(id) else {
                continue;
            };
            for idx in 0..count {
                if let Some(call_id) = summary.calls().get(idx).copied() {
                    summary.collect_sinks_for_call(stream, plan, &mut self.paths, call_id);
                }
            }
        }
    }

    fn propagate_sinks(&mut self, stream: &FactStream<Frozen>) {
        let mut reverse_calls: BTreeMap<FunctionId, Vec<FunctionId>> = BTreeMap::new();
        for (caller_id, summary) in self.by_id.iter() {
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

        let mut worklist: BTreeSet<FunctionId> = self.by_id.iter().map(|(id, _)| id).collect();

        for _ in 0..super::MAX_SUMMARY_ROUNDS {
            if worklist.is_empty() {
                break;
            }

            let current_round: Vec<FunctionId> = worklist.iter().copied().collect();
            worklist.clear();

            let mut changed: BTreeSet<FunctionId> = BTreeSet::new();

            for &caller in &current_round {
                let call_count = self
                    .by_id
                    .get(caller)
                    .map_or(0, |summary| summary.calls().len());
                for index in 0..call_count {
                    let Some(call_id) = self
                        .by_id
                        .get(caller)
                        .and_then(|summary| summary.calls().get(index))
                        .copied()
                    else {
                        continue;
                    };
                    if self.propagate_call_sinks(call_id, caller, stream) {
                        changed.insert(caller);
                    }
                }
            }

            for &changed_id in &changed {
                if let Some(summary) = self.by_id.get_mut(changed_id) {
                    summary.set_sinks_offset(summary.sinks().len());
                }
            }

            for &changed_id in &changed {
                if let Some(callers) = reverse_calls.get(&changed_id) {
                    for &c in callers {
                        worklist.insert(c);
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
    ) -> bool {
        let Some((target, args)) = resolve_call_target(call_id, stream) else {
            return false;
        };
        let target_sinks_offset = self
            .by_id
            .get(target)
            .map_or(0, FunctionSummary::sinks_offset);
        if target == caller {
            return false;
        }
        let Some((target_summary, caller_summary)) = self.by_id.get_disjoint(target, caller) else {
            return false;
        };
        let target_summary = match target_summary {
            Some(s) if s.is_invocation_compatible(stream, args, &self.paths) => s,
            _ => return false,
        };
        let Some(caller_summary) = caller_summary else {
            return false;
        };
        self.scratch_projections.clear();
        {
            let target_params = stream.function_parameters(target);
            let caller_params = stream.function_parameters(caller);
            let sink_count = target_summary.sinks().len();
            for sink_idx in target_sinks_offset..sink_count {
                let sink = target_summary.sinks().get(sink_idx).expect("valid index");
                if let Some(proj) = try_project_sink(
                    target_params,
                    caller_params,
                    sink,
                    stream,
                    args,
                    &self.paths,
                ) {
                    self.scratch_projections.push(proj);
                }
            }
        }
        let mut changed = false;
        for proj in self.scratch_projections.drain(..) {
            changed |= caller_summary.add_sink(proj);
        }
        changed
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
        parameter.parameter_index == sink.parameter_index()
            && (SummaryPathStore::matches_frozen(sink.path(), parameter.path)
                || (parameter.rest && paths.starts_with_frozen(sink.path(), parameter.path)))
    })?;
    let argument = target_parameter.project_argument_at(stream, args, paths, sink.path())?;
    let caller_parameter = caller_parameters.iter().find(|parameter| {
        !parameter.rest && parameter.value != ValueId::UNKNOWN && parameter.value == argument
    })?;
    let caller_path = paths.resolve_frozen(caller_parameter.path)?;
    Some(FunctionSinkSummary::new(
        sink.flow(),
        caller_parameter.parameter_index,
        caller_path,
    ))
}

#[cfg(test)]
mod tests {
    use crate::analysis::{
        facts,
        flow::{effect::FunctionEffects, plan::BoundFlowPlan},
        resolution::Resolver,
    };

    #[test]
    fn same_name_siblings_are_keyed_by_function_id() {
        let source = "function first(x) { document.body.appendChild(x); } function second(x) { console.log(x); }";
        let parsed = crate::parse(source, "summary-siblings.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, source);
        let stream = facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let plan = BoundFlowPlan::new(&[], stream.names());
        let summaries = super::FunctionSummaries::collect(&stream, &effects, &plan);
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
}
