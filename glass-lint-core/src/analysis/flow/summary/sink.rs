use indexmap::IndexSet;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen, ParameterBinding},
    flow::{
        planning::BoundFlowPlan,
        summary::{SummaryPathStore, store::SummaryPathId},
    },
    model::flow::FlowId,
    value::{FunctionId, ValueId},
};

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
                    self.add_sink(FunctionSinkSummary::new(
                        *flow_id,
                        parameter.parameter_index,
                        path,
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::PathId;

    use super::*;
    use crate::{
        analysis::{
            facts::{CallArgInfo, FactPayload, build_test_stream},
            flow::summary::SummaryPathStore,
            model::{fact::Frozen, flow::FlowId},
            resolution::Resolver,
            value::FunctionId,
        },
        api::classification::RuleIndex,
    };

    fn make_stream(source: &str) -> FactStream<Frozen> {
        let parsed = crate::parse(source, "sink-test.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, source);
        build_test_stream(&parsed.program, &mut resolver)
    }

    fn ri(value: usize) -> RuleIndex {
        RuleIndex::new(value)
    }

    fn extract_call_args(source: &str) -> (FactStream<Frozen>, FunctionId, Vec<CallArgInfo>) {
        let stream = make_stream(source);
        let fact = stream
            .facts()
            .iter()
            .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .cloned()
            .expect("call fact should exist");
        let (target, args) = match &fact.payload {
            FactPayload::Call {
                args: a,
                target_function,
                ..
            } => (
                target_function.expect("target function should be resolved"),
                a.clone(),
            ),
            _ => unreachable!(),
        };
        (stream, target, args)
    }

    // ── SinkSet ───────────────────────────────────────────────────────────

    #[test]
    fn sink_set_default_is_empty() {
        let set = SinkSet::default();
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn sink_set_push_unique_adds_new_sinks() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_path_id(PathId::from_raw(0));

        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 1, sp0);
        assert!(set.push_unique(s1));
        assert_eq!(set.len(), 1);
        assert!(set.push_unique(s2));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn sink_set_push_unique_rejects_duplicates() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_path_id(PathId::from_raw(0));
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        assert!(set.push_unique(s1.clone()));
        assert!(!set.push_unique(s1));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn sink_set_get_returns_sink_by_index() {
        let mut set = SinkSet::default();
        let sp1 = SummaryPathId::from_path_id(PathId::from_raw(1));
        let sp2 = SummaryPathId::from_path_id(PathId::from_raw(2));
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp1);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 1, sp2);
        set.push_unique(s1.clone());
        set.push_unique(s2.clone());
        assert_eq!(set.get(0), Some(&s1));
        assert_eq!(set.get(1), Some(&s2));
        assert!(set.get(2).is_none());
    }

    #[test]
    fn sink_set_sort_and_dedup_orders_by_flow_parameter_path() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_path_id(PathId::from_raw(0));
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 1), 1, sp0);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        set.push_unique(s2.clone());
        set.push_unique(s1.clone());
        set.sort_and_dedup();
        assert_eq!(set.len(), 2);
        assert_eq!(set.get(0), Some(&s1));
        assert_eq!(set.get(1), Some(&s2));
    }

    #[test]
    fn sink_set_into_iteration() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_path_id(PathId::from_raw(0));
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        set.push_unique(s1);
        assert_eq!(set.into_iter().count(), 1);
    }

    // ── FunctionSinkSummary ───────────────────────────────────────────────

    #[test]
    fn function_sink_summary_accessors() {
        let sp4 = SummaryPathId::from_path_id(PathId::from_raw(4));
        let flow = FlowId::new(ri(1), 2);
        let sink = FunctionSinkSummary::new(flow, 3, sp4);
        assert_eq!(sink.flow(), flow);
        assert_eq!(sink.parameter_index(), 3);
        assert_eq!(sink.path(), sp4);
    }

    // ── FunctionSummary ───────────────────────────────────────────────────

    #[test]
    fn function_summary_new_and_basic_accessors() {
        let summary = FunctionSummary::new(FunctionId(5), 3, true, vec![]);
        assert_eq!(summary.id(), FunctionId(5));
        assert_eq!(summary.parameter_count(), 3);
        assert!(summary.calls().is_empty());
        assert_eq!(summary.sinks().len(), 0);
        assert_eq!(summary.sinks_offset(), 0);
    }

    #[test]
    fn function_summary_add_sink_and_sort() {
        let sp0 = SummaryPathId::from_path_id(PathId::from_raw(0));
        let sp1 = SummaryPathId::from_path_id(PathId::from_raw(1));
        let mut summary = FunctionSummary::new(FunctionId(1), 1, false, vec![]);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(1), 0), 0, sp1);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        summary.add_sink(s2);
        summary.add_sink(s1);
        summary.sort_sinks();
        assert_eq!(summary.sinks().len(), 2);
    }

    #[test]
    fn function_summary_set_sinks_offset() {
        let mut summary = FunctionSummary::new(FunctionId(1), 0, false, vec![]);
        assert_eq!(summary.sinks_offset(), 0);
        summary.set_sinks_offset(5);
        assert_eq!(summary.sinks_offset(), 5);
    }

    // ── is_invocation_compatible ──────────────────────────────────────────

    #[test]
    fn is_invocation_compatible_accepts_matching_args() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(1);");
        let summary = FunctionSummary::new(target, 1, false, vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_spread_args() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(...x);");
        let summary = FunctionSummary::new(target, 1, false, vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_too_many_args_without_rest() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(1, 2, 3);");
        let summary = FunctionSummary::new(target, 1, false, vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_accepts_rest_param_allowing_extra_args() {
        let (stream, target, args) =
            extract_call_args("function f(...args) { return args; } f(1, 2, 3);");
        let summary = FunctionSummary::new(target, 0, true, vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_missing_required_arg() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f();");
        let summary = FunctionSummary::new(target, 1, false, vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }
}
