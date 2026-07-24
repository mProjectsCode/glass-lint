use indexmap::IndexSet;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen, ParameterBinding},
    flow::{
        index::FlowId,
        plan::BoundFlowPlan,
        summary::store::SummaryPathId,
    },
    value::{FunctionId, ValueId},
};

use super::store::SummaryPathStore;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    set: IndexSet<FunctionSinkSummary>,
}

impl SinkSet {
    pub(super) fn push_unique(&mut self, sink: FunctionSinkSummary) -> bool {
        self.set.insert(sink)
    }

    pub(super) fn len(&self) -> usize {
        self.set.len()
    }

    pub(super) fn get(&self, index: usize) -> Option<&FunctionSinkSummary> {
        self.set.get_index(index)
    }

    pub(super) fn sort_and_dedup(&mut self) {
        self.set.sort_by(|left, right| {
            (left.flow(), left.parameter_index(), left.path()).cmp(&(
                right.flow(),
                right.parameter_index(),
                right.path(),
            ))
        });
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
pub(in crate::analysis::flow) struct FunctionSummary {
    id: FunctionId,
    parameter_count: usize,
    has_rest: bool,
    calls: Vec<FactId>,
    sinks: SinkSet,
    sinks_offset: usize,
}

impl FunctionSummary {
    pub(super) fn new(
        id: FunctionId,
        parameter_count: usize,
        has_rest: bool,
        calls: Vec<FactId>,
    ) -> Self {
        Self {
            id,
            parameter_count,
            has_rest,
            calls,
            sinks: SinkSet::default(),
            sinks_offset: 0,
        }
    }

    pub(in crate::analysis::flow) fn parameter_bindings<'s>(
        &self,
        stream: &'s FactStream<Frozen>,
    ) -> &'s [ParameterBinding] {
        stream.function_parameters(self.id)
    }

    pub(in crate::analysis::flow) fn sinks(&self) -> &SinkSet {
        &self.sinks
    }

    pub(super) fn calls(&self) -> &[FactId] {
        &self.calls
    }

    pub(super) fn sinks_offset(&self) -> usize {
        self.sinks_offset
    }

    pub(super) fn set_sinks_offset(&mut self, value: usize) {
        self.sinks_offset = value;
    }

    pub(super) fn id(&self) -> FunctionId {
        self.id
    }

    #[cfg(test)]
    pub(super) fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    pub(super) fn add_sink(&mut self, sink: FunctionSinkSummary) -> bool {
        self.sinks.push_unique(sink)
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
        if args.iter().any(|argument| argument.spread) {
            return false;
        }
        if !self.has_rest && args.len() > self.parameter_count {
            return false;
        }
        for argument in args.iter().take(self.parameter_count) {
            if argument.value == ValueId::UNKNOWN {
                return false;
            }
        }
        for parameter in self.parameter_bindings(stream) {
            if parameter.rest || parameter.parameter_index >= args.len() {
                if parameter.parameter_index >= args.len()
                    && parameter.default.is_none()
                    && !parameter.rest
                {
                    return false;
                }
                continue;
            }
            if parameter.path.is_empty() {
                continue;
            }
            if parameter.project_argument(stream, args, paths).is_none()
                && parameter.default.is_none()
            {
                return false;
            }
        }
        true
    }
}

impl FunctionSummary {
    pub(super) fn collect_sinks_for_call(
        &mut self,
        stream: &FactStream<Frozen>,
        plan: &BoundFlowPlan<'_>,
        paths: &mut SummaryPathStore<'_>,
        call_id: FactId,
    ) {
        let Some(FactPayload::Call {
            syntactic_path,
            rooted_chain,
            args,
            ..
        }) = stream.fact(call_id).map(|fact| &fact.payload)
        else {
            return;
        };
        let Some(chain) = rooted_chain.as_ref().or(syntactic_path.as_ref()) else {
            return;
        };
        for flow_id in plan.sink_ids(chain).into_iter().flatten() {
            let Some(flow) = plan.get(*flow_id) else {
                continue;
            };
            let sink_members = plan.sink_member_calls(*flow_id);
            for (i, sink) in flow.sinks.iter().enumerate() {
                if !sink_members
                    .get(i)
                    .is_some_and(|members| members.iter().any(|member| member == chain))
                {
                    continue;
                }
                for argument_index in sink.args.present_indices(args.len()) {
                    let Some(argument) = args.get(argument_index) else {
                        continue;
                    };
                    let Some(parameter) =
                        self.parameter_bindings(stream).iter().find(|parameter| {
                            parameter.value != ValueId::UNKNOWN
                                && parameter.value == argument.base_value
                        })
                    else {
                        continue;
                    };
                    let Some(prefix_id) = paths.intern_frozen(parameter.path) else {
                        continue;
                    };
                    let Some(suffix_id) = paths.intern_frozen(argument.base_path) else {
                        continue;
                    };
                    let Some(path) = paths.join(prefix_id, suffix_id) else {
                        continue;
                    };
                    self.add_sink(FunctionSinkSummary::new(*flow_id, parameter.parameter_index, path));
                }
            }
        }
    }
}
