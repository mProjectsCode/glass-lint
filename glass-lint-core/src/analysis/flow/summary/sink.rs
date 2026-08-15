use glass_lint_datastructures::FastIndexSet;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactStream, Frozen, ParameterBinding},
    flow::{
        planning::BoundFlowPlan,
        summary::{SummaryPathStore, store::SummaryPathId},
    },
    model::{flow::FlowId, scope::FunctionId, value::ValueId},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis::flow) struct FunctionSinkSummary {
    flow: FlowId,
    parameter_index: usize,
    path: SummaryPathId,
}

impl FunctionSinkSummary {
    pub(super) fn new(flow: FlowId, parameter_index: usize, path: SummaryPathId) -> Self {
        Self {
            flow,
            parameter_index,
            path,
        }
    }

    pub(in crate::analysis::flow) fn flow(&self) -> FlowId {
        self.flow
    }

    pub(in crate::analysis::flow) fn parameter_index(&self) -> usize {
        self.parameter_index
    }

    pub(in crate::analysis::flow) fn path(&self) -> SummaryPathId {
        self.path
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::analysis::flow) struct SinkSet {
    set: FastIndexSet<FunctionSinkSummary>,
}

impl SinkSet {
    pub(super) fn extend_unique(&mut self, sinks: impl IntoIterator<Item = FunctionSinkSummary>) {
        for sink in sinks {
            self.set.insert(sink);
        }
    }

    fn new_count(&self, sinks: &[FunctionSinkSummary]) -> usize {
        let mut pending = Vec::new();
        for sink in sinks {
            if !self.set.contains(sink) && !pending.contains(&sink) {
                pending.push(sink);
            }
        }
        pending.len()
    }

    pub(super) fn sort_and_dedup(&mut self) {
        self.set.sort();
    }
}

impl<'a> IntoIterator for &'a SinkSet {
    type IntoIter = indexmap::set::Iter<'a, FunctionSinkSummary>;
    type Item = &'a FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.set.iter()
    }
}

impl IntoIterator for SinkSet {
    type IntoIter = indexmap::set::IntoIter<FunctionSinkSummary>;
    type Item = FunctionSinkSummary;

    fn into_iter(self) -> Self::IntoIter {
        self.set.into_iter()
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis::flow) struct FunctionSignature {
    parameter_count: usize,
    has_rest: bool,
}

impl FunctionSignature {
    pub(super) fn new(parameter_count: usize, has_rest: bool) -> Self {
        Self {
            parameter_count,
            has_rest,
        }
    }

    pub(super) fn from_bindings(parameters: &[ParameterBinding]) -> Self {
        Self::new(
            parameters
                .iter()
                .map(ParameterBinding::parameter_index)
                .max()
                .map_or(0, |index| index.saturating_add(1)),
            parameters.iter().any(ParameterBinding::is_rest),
        )
    }

    fn accepts_call_shape(&self, args: &[CallArgInfo]) -> bool {
        if args.iter().any(|argument| argument.spread)
            || (!self.has_rest && args.len() > self.parameter_count)
        {
            return false;
        }
        args.iter()
            .take(self.parameter_count)
            .all(|argument| argument.value != ValueId::UNKNOWN)
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis::flow) struct FunctionSummary {
    id: FunctionId,
    signature: FunctionSignature,
    calls: Vec<FactId>,
    sinks: SinkSet,
}

impl FunctionSummary {
    pub(super) fn new(id: FunctionId, signature: FunctionSignature, calls: Vec<FactId>) -> Self {
        Self {
            id,
            signature,
            calls,
            sinks: SinkSet::default(),
        }
    }

    pub(in crate::analysis::flow) fn parameter_bindings<'s>(
        &self,
        stream: &'s FactStream<Frozen>,
    ) -> Option<&'s [ParameterBinding]> {
        stream.function_parameters(self.id)
    }

    pub(in crate::analysis::flow) fn sinks(&self) -> &SinkSet {
        &self.sinks
    }

    pub(super) fn calls(&self) -> &[FactId] {
        &self.calls
    }

    pub(super) fn id(&self) -> FunctionId {
        self.id
    }

    #[cfg(test)]
    pub(super) fn parameter_count(&self) -> usize {
        self.signature.parameter_count
    }

    pub(super) fn add_sinks(&mut self, sinks: impl IntoIterator<Item = FunctionSinkSummary>) {
        self.sinks.extend_unique(sinks);
    }

    pub(super) fn new_sink_count(&self, sinks: &[FunctionSinkSummary]) -> usize {
        self.sinks.new_count(sinks)
    }

    pub(super) fn sort_sinks(&mut self) {
        self.sinks.sort_and_dedup();
    }
}

impl FunctionSummary {
    pub(in crate::analysis::flow) fn is_invocation_compatible(
        &self,
        stream: &FactStream<Frozen>,
        args: &[CallArgInfo],
        paths: &SummaryPathStore<'_>,
    ) -> bool {
        self.signature.accepts_call_shape(args)
            && self.parameter_bindings(stream).is_some_and(|parameters| {
                parameters
                    .iter()
                    .all(|parameter| parameter.accepts_invocation_projection(stream, args, paths))
            })
    }
}

impl FunctionSummary {
    pub(super) fn collect_sinks_for_call(
        &self,
        stream: &FactStream<Frozen>,
        plan: &BoundFlowPlan<'_>,
        paths: &mut SummaryPathStore<'_>,
        call_id: FactId,
    ) -> Vec<FunctionSinkSummary> {
        let cref = stream.call_effect(call_id);
        let Some(shape) = cref.shape() else {
            return Vec::new();
        };
        let args = shape.effective_args();
        let sinks = plan.sink_candidates_for_call(&shape);
        let mut candidates = Vec::new();
        for sink in sinks.into_iter().flatten() {
            for argument_index in sink.present_indices(args.len()) {
                let Some(argument) = args.get(argument_index) else {
                    continue;
                };
                let Some(parameter) = self.parameter_bindings(stream).and_then(|parameters| {
                    parameters.iter().find(|parameter| {
                        parameter.value() != ValueId::UNKNOWN
                            && parameter.value() == argument.base_value
                    })
                }) else {
                    continue;
                };
                let Some(prefix_id) = paths.intern_frozen(parameter.path()) else {
                    continue;
                };
                let Some(suffix_id) = paths.intern_frozen(argument.base_path) else {
                    continue;
                };
                let Some(path) = paths.join(prefix_id, suffix_id) else {
                    continue;
                };
                candidates.push(FunctionSinkSummary::new(
                    sink.flow_id(),
                    parameter.parameter_index(),
                    path,
                ));
            }
        }
        candidates
    }
}

#[cfg(test)]
mod tests;
