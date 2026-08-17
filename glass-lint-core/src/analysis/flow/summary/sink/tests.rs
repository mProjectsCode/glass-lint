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
        .find(|f| matches!(f.payload(), FactPayload::Call(_)))
        .cloned()
        .expect("call fact should exist");
    let (target, args) = match fact.payload() {
        FactPayload::Call(call) => (
            call.target_function()
                .expect("target function should be resolved"),
            call.args().to_vec(),
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
    set.extend_unique([s1]);
    assert_eq!((&set).into_iter().count(), 1);
    set.extend_unique([s2]);
    assert_eq!((&set).into_iter().count(), 2);
}

#[test]
fn sink_set_push_unique_rejects_duplicates() {
    let mut set = SinkSet::default();
    let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
    let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
    set.extend_unique([s1.clone()]);
    set.extend_unique([s1]);
    assert_eq!((&set).into_iter().count(), 1);
}

#[test]
fn sink_set_extend_unique_deduplicates_candidates() {
    let (_paths, p0, p1, _p2) = test_paths();
    let mut set = SinkSet::default();
    let sp1 = SummaryPathId::from_frozen_path(p0);
    let sp2 = SummaryPathId::from_frozen_path(p1);
    let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp1);
    let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 1, sp2);
    set.extend_unique(vec![s1.clone(), s1, s2]);
    assert_eq!((&set).into_iter().count(), 2);
}

#[test]
fn sink_set_sort_and_dedup_orders_by_flow_parameter_path() {
    let mut set = SinkSet::default();
    let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
    let s2 = FunctionSinkSummary::new(FlowId::new(ri(0), 1), 1, sp0);
    let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
    set.extend_unique([s2.clone()]);
    set.extend_unique([s1.clone()]);
    set.sort_and_dedup();
    let sinks: Vec<&FunctionSinkSummary> = (&set).into_iter().collect();
    assert_eq!(sinks, vec![&s1, &s2]);
}

#[test]
fn sink_set_into_iteration() {
    let mut set = SinkSet::default();
    let sp0 = SummaryPathId::from_frozen_path(PathId::EMPTY);
    let s1 = FunctionSinkSummary::new(FlowId::new(ri(0), 0), 0, sp0);
    set.extend_unique([s1]);
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
    assert_eq!(summary.calls().len(), 0);
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
    summary.add_sinks([s2]);
    summary.add_sinks([s1]);
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
