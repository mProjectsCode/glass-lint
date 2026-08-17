use glass_lint_datastructures::FastIndexSet;

use crate::analysis::{
    facts::ParameterBinding,
    flow::summary::{SummaryPathStore, store::SummaryPathId},
    model::{flow::FlowId, value::ValueId},
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

pub(in crate::analysis::flow) fn find_sink_parameter<'a>(
    parameters: &'a [ParameterBinding],
    sink: &FunctionSinkSummary,
    paths: &SummaryPathStore<'_>,
) -> Option<&'a ParameterBinding> {
    parameters.iter().find(|parameter| {
        parameter.parameter_index() == sink.parameter_index()
            && parameter.matches_sink_path(sink.path(), paths)
    })
}

pub(in crate::analysis::flow) fn parameter_for_value(
    parameters: &[ParameterBinding],
    value: ValueId,
) -> Option<&ParameterBinding> {
    (value != ValueId::UNKNOWN)
        .then(|| {
            parameters
                .iter()
                .find(|parameter| parameter.value() == value)
        })
        .flatten()
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

    pub(super) fn new_count(&self, sinks: &[FunctionSinkSummary]) -> usize {
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

#[cfg(test)]
mod tests;
