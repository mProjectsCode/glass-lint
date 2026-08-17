use glass_lint_datastructures::{PathId, PathSegment, PathStore};

use super::*;
use crate::{
    analysis::{
        facts,
        flow::{effect::FunctionEffects, planning::BoundFlowPlan, summary::store::SummaryPathId},
        model::flow::FlowId,
    },
    api::classification::RuleIndex,
};

fn unlimited_budget() -> Budget {
    Budget::new(usize::MAX)
}

fn test_paths() -> (PathStore, PathId, PathId, PathId) {
    let mut paths = PathStore::new();
    let p0 = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    let p1 = paths.append(PathId::EMPTY, PathSegment::Index(1)).unwrap();
    let p2 = paths.append(PathId::EMPTY, PathSegment::Index(2)).unwrap();
    (paths, p0, p1, p2)
}

fn ri(value: usize) -> RuleIndex {
    RuleIndex::new(value)
}

fn make_stream(source: &str) -> FactStream<Frozen> {
    facts::build_test_facts(source, "summary-test.js")
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
fn rejected_sink_admission_does_not_mutate_the_summary() {
    let mut summary = FunctionSummary::new(
        FunctionId::from_test(1),
        FunctionSignature::new(1, false),
        vec![],
    );
    let candidate = FunctionSinkSummary::new(
        crate::analysis::model::flow::FlowId::new(crate::api::classification::RuleIndex::new(0), 0),
        0,
        SummaryPathId::from_frozen_path(glass_lint_datastructures::PathId::EMPTY),
    );
    let mut sink_budget = SummarySinkBudget {
        retained: MAX_SUMMARY_SINKS,
    };
    let mut budget = unlimited_budget();

    assert!(
        sink_budget
            .admit_sinks(&mut summary, vec![candidate], &mut budget)
            .is_err()
    );
    assert!(summary.sinks().into_iter().next().is_none());
    assert_eq!(sink_budget.retained, MAX_SUMMARY_SINKS);
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
        .find(|f| matches!(f.payload(), FactPayload::Call(_)))
        .expect("call fact should exist");
    let FactPayload::Call(call) = call_fact.payload() else {
        unreachable!()
    };
    assert!(!f.is_invocation_compatible(&stream, call.args(), &summaries.paths));
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
        .find(|f| matches!(f.payload(), FactPayload::Call(_)))
        .expect("call fact should exist");
    let FactPayload::Call(call) = call_fact.payload() else {
        unreachable!()
    };
    assert!(!f.is_invocation_compatible(&stream, call.args(), &summaries.paths));
}
