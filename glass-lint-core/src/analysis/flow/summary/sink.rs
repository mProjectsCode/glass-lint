use glass_lint_datastructures::FastIndexSet;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactStream, Frozen, ParameterBinding},
    flow::{
        planning::{BoundFlowPlan, FlowMatchView},
        summary::{SummaryPathStore, store::SummaryPathId},
    },
    model::{flow::FlowId, scope::FunctionId, value::ValueId},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::analysis::flow) struct InsertOutcome {
    inserted: usize,
}

impl InsertOutcome {
    pub(super) fn new(inserted: usize) -> Self {
        Self { inserted }
    }

    pub(super) fn inserted(self) -> usize {
        self.inserted
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::analysis::flow) struct SinkSet {
    set: FastIndexSet<FunctionSinkSummary>,
}

impl SinkSet {
    pub(super) fn push_unique(&mut self, sink: FunctionSinkSummary) -> InsertOutcome {
        InsertOutcome::new(usize::from(self.set.insert(sink)))
    }

    pub(super) fn extend_unique(
        &mut self,
        sinks: impl IntoIterator<Item = FunctionSinkSummary>,
    ) -> InsertOutcome {
        let mut inserted = 0;
        for sink in sinks {
            inserted += usize::from(self.set.insert(sink));
        }
        InsertOutcome::new(inserted)
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

    pub(super) fn clear(&mut self) {
        self.set.clear();
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
    ) -> &'s [ParameterBinding] {
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

    pub(super) fn add_sink(&mut self, sink: FunctionSinkSummary) -> InsertOutcome {
        self.sinks.push_unique(sink)
    }

    pub(super) fn add_sinks(
        &mut self,
        sinks: impl IntoIterator<Item = FunctionSinkSummary>,
    ) -> InsertOutcome {
        self.sinks.extend_unique(sinks)
    }

    pub(super) fn sort_sinks(&mut self) {
        self.sinks.sort_and_dedup();
    }

    pub(super) fn clear_sinks(&mut self) {
        self.sinks.clear();
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
            && self
                .parameter_bindings(stream)
                .iter()
                .all(|parameter| parameter.accepts_invocation_projection(stream, args, paths))
    }
}

impl FunctionSummary {
    pub(super) fn collect_sinks_for_call(
        &mut self,
        stream: &FactStream<Frozen>,
        plan: &BoundFlowPlan<'_>,
        paths: &mut SummaryPathStore<'_>,
        call_id: FactId,
    ) -> InsertOutcome {
        let cref = stream.call_effect(call_id);
        let Some(args) = cref.effective_args() else {
            return InsertOutcome::default();
        };
        let matcher = FlowMatchView::new(stream.names(), stream.values());
        let flow_ids = cref
            .global_name()
            .and_then(|name| plan.global_sink_ids(name))
            .or_else(|| cref.chain().and_then(|chain| plan.sink_ids(chain)));
        let mut candidates = Vec::new();
        for flow_id in flow_ids.into_iter().flatten() {
            let Some(flow) = plan.get(*flow_id) else {
                continue;
            };
            for sink in flow.sinks() {
                if !matcher.target_matches(
                    sink.target(),
                    cref.global_name().map(smol_str::SmolStr::as_str),
                    cref.chain(),
                    cref.rooted(),
                ) {
                    continue;
                }
                for argument_index in sink.present_indices(args.len()) {
                    let Some(argument) = args.get(argument_index) else {
                        continue;
                    };
                    let Some(parameter) =
                        self.parameter_bindings(stream).iter().find(|parameter| {
                            parameter.value() != ValueId::UNKNOWN
                                && parameter.value() == argument.base_value
                        })
                    else {
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
                        *flow_id,
                        parameter.parameter_index(),
                        path,
                    ));
                }
            }
        }
        self.add_sinks(candidates)
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{PathId, PathSegment, PathStore};

    use super::*;
    use crate::{
        analysis::{
            facts::{CallArgInfo, FactPayload, build_test_facts},
            flow::summary::SummaryPathStore,
            model::{fact::Frozen, flow::FlowId, scope::FunctionId},
        },
        api::classification::RuleIndex,
    };

    fn test_paths() -> (PathStore, PathId, PathId, PathId) {
        let mut paths = PathStore::new();
        let p0 = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
        let p1 = paths.append(PathId::EMPTY, PathSegment::Index(1)).unwrap();
        let p2 = paths.append(PathId::EMPTY, PathSegment::Index(2)).unwrap();
        (paths, p0, p1, p2)
    }

    fn make_stream(source: &str) -> FactStream<Frozen> {
        build_test_facts(source, "sink-test.js")
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
        assert_eq!(set.into_iter().count(), 0);
    }

    #[test]
    fn sink_set_push_unique_adds_new_sinks() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);

        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 1, sp0);
        assert_eq!(set.push_unique(s1).inserted(), 1);
        assert_eq!((&set).into_iter().count(), 1);
        assert_eq!(set.push_unique(s2).inserted(), 1);
        assert_eq!((&set).into_iter().count(), 2);
    }

    #[test]
    fn sink_set_push_unique_rejects_duplicates() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        assert_eq!(set.push_unique(s1.clone()).inserted(), 1);
        assert_eq!(set.push_unique(s1).inserted(), 0);
        assert_eq!((&set).into_iter().count(), 1);
    }

    #[test]
    fn sink_set_extend_unique_reports_total_inserted_after_dedup() {
        let (_paths, p0, p1, _p2) = test_paths();
        let mut set = SinkSet::default();
        let sp1 = SummaryPathId::from_frozen_path(p0);
        let sp2 = SummaryPathId::from_frozen_path(p1);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp1);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 1, sp2);
        let outcome = set.extend_unique(vec![s1.clone(), s1, s2]);
        assert_eq!(outcome.inserted(), 2);
        assert_eq!((&set).into_iter().count(), 2);
    }

    #[test]
    fn sink_set_sort_and_dedup_orders_by_flow_parameter_path() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 1), 1, sp0);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        set.push_unique(s2.clone());
        set.push_unique(s1.clone());
        set.sort_and_dedup();
        let sinks: Vec<&FunctionSinkSummary> = (&set).into_iter().collect();
        assert_eq!(sinks, vec![&s1, &s2]);
    }

    #[test]
    fn sink_set_into_iteration() {
        let mut set = SinkSet::default();
        let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        set.push_unique(s1);
        assert_eq!(set.into_iter().count(), 1);
    }

    // ── FunctionSinkSummary ───────────────────────────────────────────────

    #[test]
    fn function_sink_summary_accessors() {
        let (_paths, _p0, p1, _p2) = test_paths();
        let sp = SummaryPathId::from_frozen_path(p1);
        let flow = FlowId::new(ri(1), 2);
        let sink = FunctionSinkSummary::new(flow, 3, sp);
        assert_eq!(sink.flow(), flow);
        assert_eq!(sink.parameter_index(), 3);
        assert_eq!(sink.path(), sp);
    }

    // ── FunctionSummary ───────────────────────────────────────────────────

    #[test]
    fn function_summary_new_and_basic_accessors() {
        let summary = FunctionSummary::new(
            FunctionId::from_test(5),
            FunctionSignature::new(3, true),
            vec![],
        );
        assert_eq!(summary.id(), FunctionId::from_test(5));
        assert_eq!(summary.parameter_count(), 3);
        assert!(summary.calls().is_empty());
        assert_eq!(summary.sinks().into_iter().count(), 0);
    }

    #[test]
    fn function_summary_add_sink_and_sort() {
        let (_paths, p0, p1, _p2) = test_paths();
        let sp0 = SummaryPathId::from_frozen_path(p0);
        let sp1 = SummaryPathId::from_frozen_path(p1);
        let mut summary = FunctionSummary::new(
            FunctionId::from_test(1),
            FunctionSignature::new(1, false),
            vec![],
        );
        let s1 = FunctionSinkSummary::new(FlowId::new(ri(1), 0), 0, sp1);
        let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
        summary.add_sink(s2);
        summary.add_sink(s1);
        summary.sort_sinks();
        assert_eq!(summary.sinks().into_iter().count(), 2);
    }

    // ── is_invocation_compatible ──────────────────────────────────────────

    #[test]
    fn is_invocation_compatible_accepts_matching_args() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(1);");
        let summary = FunctionSummary::new(target, FunctionSignature::new(1, false), vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_spread_args() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(...x);");
        let summary = FunctionSummary::new(target, FunctionSignature::new(1, false), vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_too_many_args_without_rest() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f(1, 2, 3);");
        let summary = FunctionSummary::new(target, FunctionSignature::new(1, false), vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_accepts_rest_param_allowing_extra_args() {
        let (stream, target, args) =
            extract_call_args("function f(...args) { return args; } f(1, 2, 3);");
        let summary = FunctionSummary::new(target, FunctionSignature::new(0, true), vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(summary.is_invocation_compatible(&stream, &args, &paths));
    }

    #[test]
    fn is_invocation_compatible_rejects_missing_required_arg() {
        let (stream, target, args) = extract_call_args("function f(a) { return a; } f();");
        let summary = FunctionSummary::new(target, FunctionSignature::new(1, false), vec![]);
        let paths = SummaryPathStore::new(stream.paths());
        assert!(!summary.is_invocation_compatible(&stream, &args, &paths));
    }
}
