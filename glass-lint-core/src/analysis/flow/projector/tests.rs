use super::*;
use crate::{
    analysis::trace::TraceArena,
    api::{
        classification::RuleEvidenceTable,
        compiler::{normalize::normalize_query_decl, normalized::NormalizedRoot},
        rule::{
            EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
            LifecycleSink, QueryDecl, ValueMatcher,
        },
    },
    project::ModuleId,
};

fn collect_with_limits_test(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[(RuleIndex, usize, &CompiledObjectFlow)],
    rule_count: usize,
    limits: FlowLimits,
) -> (RuleEvidenceTable, LocalFlowProjectionOutcome) {
    let mut arena = TraceArena::new(4096);
    collect_with_limits(
        stream,
        effects,
        rules,
        rule_count,
        limits,
        ModuleId::new(0),
        &mut arena,
    )
}

fn compile_flow(query: &LifecycleQuery) -> CompiledObjectFlow {
    let declaration = QueryDecl::lifecycle(Ok(query.clone())).expect("lifecycle should build");
    let normalized = normalize_query_decl(&declaration).expect("lifecycle should normalize");
    let NormalizedRoot::Lifecycle(lifecycle) = normalized.root() else {
        panic!("lifecycle declaration should normalize to a lifecycle root");
    };
    CompiledObjectFlow::from_normalized_lifecycle(lifecycle, normalized.emission().symbol())
        .expect("validated lifecycle should have valid sources")
}

fn collect_source(source: &str, query: &LifecycleQuery) -> RuleEvidenceTable {
    let stream = crate::analysis::facts::build_test_facts(source, "fact-flow.js");
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(query);
    let (evidence, _outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        FlowLimits::from_flow_operations(262_144),
    );
    evidence
}

fn collect_source_with_outcome(
    source: &str,
    query: &LifecycleQuery,
    limits: FlowLimits,
) -> LocalFlowProjectionOutcome {
    let stream = crate::analysis::facts::build_test_facts(source, "flow-metrics.js");
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(query);
    let (_evidence, outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    outcome
}

#[test]
fn path_batches_reject_tokens_from_another_generation() {
    let mut frontier = PathFrontier::initial();
    let first = frontier.begin_batch(2);
    let first_path = first.token(1).expect("path is within the first batch");
    assert!(first.contains(first_path));
    assert!(frontier.select_path(1));
    assert_eq!(frontier.active_path(), Some(first_path));
    frontier.end_batch();

    let second = frontier.begin_batch(1);
    assert_ne!(first.generation, second.generation);
    assert!(!second.contains(first_path));
    assert!(second.token(1).is_none());
}

#[test]
fn empty_flow_catalog_skips_projection_work() {
    let stream = crate::analysis::facts::build_test_facts("fetch('/api');", "no-flow.js");
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let mut arena = TraceArena::new(4096);
    let mut evidence = RuleEvidenceTable::new_for_test(1);

    let outcome = collect_into(
        &stream,
        &effects,
        &[],
        &mut evidence,
        FlowLimits::from_flow_operations(262_144),
        ModuleId::new(0),
        &mut arena,
    );

    assert_eq!(outcome.operations, 0);
    assert_eq!(evidence[0].len(), 0);
}

fn script_flow() -> LifecycleQuery {
    LifecycleQuery::catalog_builder("script insertion")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(
                    0,
                    ValueMatcher::static_string().try_equals("script").unwrap(),
                ),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("document.head.appendChild", 0),
        ]))
        .build()
        .unwrap()
}

#[test]
fn transfers_source_configuration_and_sink_from_facts() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
}

#[test]
fn flow_metrics_charge_path_and_trace_work() {
    let outcome = collect_source_with_outcome(
        "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);",
        &script_flow(),
        FlowLimits::from_flow_operations(262_144),
    );
    assert!(outcome.operations > 0);
    assert!(outcome.max_live_alternatives >= 1);
    assert!(outcome.trace_heads >= 1);
    assert!(!outcome.is_exhausted());
}

#[test]
fn flow_metrics_are_repeatable_for_the_same_source_and_limits() {
    let source = "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);";
    let limits = FlowLimits::from_flow_operations(262_144);
    let first = collect_source_with_outcome(source, &script_flow(), limits);
    let second = collect_source_with_outcome(source, &script_flow(), limits);
    assert_eq!(first.operations, second.operations);
    assert_eq!(first.max_live_alternatives, second.max_live_alternatives);
    assert_eq!(first.coalescing_comparisons, second.coalescing_comparisons);
    assert_eq!(first.fixed_point_iterations, second.fixed_point_iterations);
    assert_eq!(first.trace_heads, second.trace_heads);
    assert_eq!(first.is_exhausted(), second.is_exhausted());
}

#[test]
fn equivalent_branch_paths_are_coalesced_and_counted() {
    let outcome = collect_source_with_outcome(
        "const script = document.createElement('script'); if (flag) { script.src = url; } else { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
        FlowLimits::from_flow_operations(262_144),
    );
    assert!(outcome.coalescing_comparisons > 0);
    assert!(outcome.max_live_alternatives >= 2);
    assert!(!outcome.is_exhausted());
}

#[test]
fn loop_fixed_point_iterations_are_bounded_and_visible() {
    let outcome = collect_source_with_outcome(
        "const script = document.createElement('script'); while (flag) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
        FlowLimits::from_flow_operations(262_144),
    );
    assert!(outcome.fixed_point_iterations > 0);
    assert!(outcome.operations >= outcome.fixed_point_iterations);
    assert!(!outcome.is_exhausted());
}

#[test]
fn exhausted_flow_operation_budget_is_reported_as_incomplete() {
    let outcome = collect_source_with_outcome(
        "const script = document.createElement('script'); if (flag) { script.src = url; } else { script.src = other; } document.head.appendChild(script);",
        &script_flow(),
        FlowLimits::test_with_operation_limit(65_536, 262_144, 65_536, 4096, 1),
    );
    assert!(outcome.is_exhausted());
    assert!(outcome.operations <= 1);
}

#[test]
fn summary_completion_reason_reaches_local_outcome() {
    let outcome = collect_source_with_outcome(
        "function helper(value) { document.head.appendChild(value); } const script = document.createElement('script'); script.src = url; helper(script);",
        &script_flow(),
        FlowLimits::test_new(65_536, 262_144, 1, 4096),
    );

    assert_eq!(
        outcome.completion,
        FlowCompletion::incomplete(FlowCompletionReason::SummaryBudget)
    );
}

#[test]
fn member_call_configuration_stays_with_its_receiver() {
    let flow = LifecycleQuery::catalog_builder("configured script")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(
                    0,
                    ValueMatcher::static_string().try_equals("script").unwrap(),
                ),
        )
        .condition(LifecycleCondition::event(
            LifecycleEvent::member_call("configure")
                .unwrap()
                .arg(0, ValueMatcher::static_string().try_equals("yes").unwrap())
                .unwrap(),
        ))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("document.head.appendChild", 0),
        ]))
        .build()
        .unwrap();
    let evidence = collect_source(
        "const first = document.createElement('script'); const second = document.createElement('script'); first.configure('yes'); document.head.appendChild(second); document.head.appendChild(first);",
        &flow,
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
}

#[test]
fn property_invalidation_is_driven_by_assignment_facts() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; script.src += suffix; document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].len(), 0);
}

#[test]
fn separate_sink_facts_produce_separate_match_occurrences() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        2
    );
}

#[test]
fn unchanged_branch_paths_retain_baseline_state() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; if (ready) {} document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
}

#[test]
fn identical_branch_requirements_are_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); if (ready) { script.src = url; } else { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        2
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Definite)
    );
}

#[test]
fn one_arm_requirement_does_not_leak_after_join() {
    let evidence = collect_source(
        "const script = document.createElement('script'); if (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn zero_iteration_loops_do_not_make_body_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); while (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn do_while_body_configuration_is_reachable_after_loop() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { script.src = url; } while (ready); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Definite)
    );
}

#[test]
fn continue_is_a_loop_back_edge_not_a_post_loop_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); while (ready) { script.src = url; continue; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn for_loop_update_reaches_the_fixed_point_without_unrolling_runtime_paths() {
    let evidence = collect_source(
        "const script = document.createElement('script'); for (; ready; index++) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn for_in_and_for_of_include_the_zero_iteration_path() {
    for loop_statement in [
        "for (const item in items) { script.src = url; }",
        "for (const item of items) { script.src = url; }",
    ] {
        let source = format!(
            "const script = document.createElement('script'); {loop_statement} document.head.appendChild(script);"
        );
        let evidence = collect_source(&source, &script_flow());
        assert_eq!(
            evidence[0]
                .iter()
                .map(ClassificationEvidence::count)
                .sum::<u32>(),
            1
        );
        assert!(
            evidence[0]
                .iter()
                .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
        );
    }
}

#[test]
fn repeated_loop_source_rebinding_does_not_accumulate_unreachable_states() {
    let evidence = collect_source(
        "let script; while (ready) { script = document.createElement('script'); script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn catch_only_configuration_does_not_become_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); try { work(); } catch (error) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(
        evidence[0]
            .iter()
            .map(ClassificationEvidence::count)
            .sum::<u32>(),
        1
    );
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty() == crate::project::MatchCertainty::Possible)
    );
}

#[path = "tests_extended.rs"]
mod tests_extended;
