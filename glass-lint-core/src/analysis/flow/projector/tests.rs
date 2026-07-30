use super::*;
use crate::{
    analysis::{resolution::Resolver, trace::TraceArena},
    api::{
        compiler::{normalize::normalize_query_decl, normalized::NormalizedRoot},
        rule::{
            LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery, LifecycleSink,
            LifecycleSource, QueryDecl, ValueMatcher,
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
) -> (Vec<Vec<ClassificationEvidence>>, LocalFlowProjectionOutcome) {
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
}

fn collect_source(source: &str, query: &LifecycleQuery) -> Vec<Vec<ClassificationEvidence>> {
    let parsed = crate::parse(source, "fact-flow.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
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
    let parsed = crate::parse(source, "flow-metrics.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
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
fn empty_flow_catalog_skips_projection_work() {
    let parsed = crate::parse("fetch('/api');", "no-flow.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, "fetch('/api');");
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let mut arena = TraceArena::new(4096);
    let mut evidence = vec![Vec::new()];

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
    assert!(evidence[0].is_empty());
}

fn script_flow() -> LifecycleQuery {
    LifecycleQuery::builder("script insertion")
        .source(
            LifecycleSource::returned_by("document.createElement")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals("script")),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([LifecycleSink::argument_of(
            "document.head.appendChild",
            0,
        )]))
        .build()
        .unwrap()
}

#[test]
fn transfers_source_configuration_and_sink_from_facts() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
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
    assert!(!outcome.exhausted);
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
    assert!(!outcome.exhausted);
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
    assert!(!outcome.exhausted);
}

#[test]
fn exhausted_flow_operation_budget_is_reported_as_incomplete() {
    let outcome = collect_source_with_outcome(
        "const script = document.createElement('script'); if (flag) { script.src = url; } else { script.src = other; } document.head.appendChild(script);",
        &script_flow(),
        FlowLimits::test_with_operation_limit(65_536, 262_144, 65_536, 4096, 1),
    );
    assert!(outcome.exhausted);
    assert!(outcome.operations <= 1);
}

#[test]
fn member_call_configuration_stays_with_its_receiver() {
    let flow = LifecycleQuery::builder("configured script")
        .source(
            LifecycleSource::returned_by("document.createElement")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals("script")),
        )
        .condition(LifecycleCondition::event(
            LifecycleEvent::member_call("configure")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals("yes"))
                .unwrap()
                .build(),
        ))
        .completion(LifecycleCompletion::any_sink([LifecycleSink::argument_of(
            "document.head.appendChild",
            0,
        )]))
        .build()
        .unwrap();
    let evidence = collect_source(
        "const first = document.createElement('script'); const second = document.createElement('script'); first.configure('yes'); document.head.appendChild(second); document.head.appendChild(first);",
        &flow,
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn property_invalidation_is_driven_by_assignment_facts() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; script.src += suffix; document.head.appendChild(script);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn separate_sink_facts_produce_separate_match_occurrences() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 2);
}

#[test]
fn unchanged_branch_paths_retain_baseline_state() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; if (ready) {} document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn identical_branch_requirements_are_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); if (ready) { script.src = url; } else { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 2);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Definite)
    );
}

#[test]
fn one_arm_requirement_does_not_leak_after_join() {
    let evidence = collect_source(
        "const script = document.createElement('script'); if (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn zero_iteration_loops_do_not_make_body_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); while (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn do_while_body_configuration_is_reachable_after_loop() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { script.src = url; } while (ready); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Definite)
    );
}

#[test]
fn continue_is_a_loop_back_edge_not_a_post_loop_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); while (ready) { script.src = url; continue; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn for_loop_update_reaches_the_fixed_point_without_unrolling_runtime_paths() {
    let evidence = collect_source(
        "const script = document.createElement('script'); for (; ready; index++) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
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
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
        assert!(
            evidence[0]
                .iter()
                .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
        );
    }
}

#[test]
fn repeated_loop_source_rebinding_does_not_accumulate_unreachable_states() {
    let evidence = collect_source(
        "let script; while (ready) { script = document.createElement('script'); script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn catch_only_configuration_does_not_become_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); try { work(); } catch (error) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn catch_sink_can_consume_a_source_from_before_try() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; try { work(); } catch (error) { document.head.appendChild(script); }",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn finally_configuration_is_applied_to_normal_completion() {
    let evidence = collect_source(
        "const script = document.createElement('script'); try { work(); } finally { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn switch_no_match_path_prevents_case_only_configuration() {
    let evidence = collect_source(
        "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn default_case_can_make_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; default: script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 2);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Definite)
    );
}

#[test]
fn incompatible_branch_facts_do_not_form_a_flow_witness() {
    let evidence = collect_source(
        "const script = document.createElement('script'); let inserted; if (ready) { script.src = url; inserted = localElement; } else { inserted = script; } document.head.appendChild(inserted);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn source_created_on_one_branch_can_reach_a_possible_sink_after_join() {
    let evidence = collect_source(
        "let script; if (ready) { script = document.createElement('script'); script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    assert!(
        evidence[0]
            .iter()
            .all(|item| item.certainty == crate::project::MatchCertainty::Possible)
    );
}

#[test]
fn do_while_break_preserves_the_break_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { script.src = url; break; } while (ready); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn finally_configuration_reaches_a_break_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { try { break; } finally { script.src = url; } } while (ready); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn finally_return_does_not_reach_code_after_the_try() {
    let evidence = collect_source(
        "function run() { const script = document.createElement('script'); try { return; } finally { script.src = url; } document.head.appendChild(script); }",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn destructuring_assignment_invalidates_the_written_alias() {
    let evidence = collect_source(
        "let script = document.createElement('script'); script.src = url; ({ script } = replacement); document.head.appendChild(script);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn rebinding_one_alias_does_not_kill_the_shared_object() {
    let evidence = collect_source(
        "let first = document.createElement('script'); const alias = first; first = replacement; alias.src = url; document.head.appendChild(alias);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn flow_evidence_is_anchored_at_the_sink_event() {
    let source = "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);";
    let parsed = crate::parse(source, "flow-location.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let sink_span = stream
        .facts()
        .iter()
        .find_map(|fact| match &fact.payload {
            FactPayload::Call {
                syntactic_path: Some(chain),
                ..
            } if stream
                .names()
                .resolve_path(chain)
                .is_some_and(|s| s.eq_chain("document.head.appendChild")) =>
            {
                Some(fact.span)
            }
            _ => None,
        })
        .expect("sink call should be present");
    let lc = script_flow();
    let flow = compile_flow(&lc);
    let (evidence, _outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        FlowLimits::from_flow_operations(262_144),
    );
    assert_eq!(evidence[0][0].occurrences[0].span, sink_span);
}

#[test]
fn requirement_only_evidence_is_anchored_at_the_configuration_event() {
    let flow = LifecycleQuery::builder("configured input")
        .source(
            LifecycleSource::returned_by("document.createElement")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals("input")),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::static_string().equals("file"),
        )))
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    let source = "const input = document.createElement('input'); input.type = 'file';";
    let parsed = crate::parse(source, "flow-requirement-location.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let configuration = stream
        .facts()
        .iter()
        .find_map(|fact| {
            matches!(fact.payload, FactPayload::PropertyWrite { .. })
                .then_some((fact.id, fact.span))
        })
        .expect("configuration write should be present");
    let flow = compile_flow(&flow);
    let (evidence, _outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        FlowLimits::from_flow_operations(262_144),
    );
    assert_eq!(evidence[0][0].occurrences[0].span, configuration.1);
    assert_eq!(evidence[0][0].occurrences[0].fact, Some(configuration.0.0));
}

#[test]
fn object_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); const b = document.createElement('script');";
    let parsed = crate::parse(source, "obj-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(&query);
    let limits = FlowLimits::test_new(1, 262_144, 65_536, 4096);
    let (evidence, outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    assert!(outcome.exhausted, "object limit should be exhausted");
    assert_eq!(
        outcome.objects_used, 1,
        "only one object should be allocated"
    );
    assert!(
        evidence[0].is_empty(),
        "no flow can complete without a second object"
    );
}

#[test]
fn mutation_log_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); const b = document.createElement('script');";
    let parsed = crate::parse(source, "mut-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(&query);
    let limits = FlowLimits::test_new(65_536, 262_144, 65_536, 1);
    let (_evidence, outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    assert!(outcome.exhausted, "mutation log limit should be exhausted");
}

#[test]
fn state_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let parsed = crate::parse(source, "state-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(&query);
    let limits = FlowLimits::test_new(65_536, 0, 65_536, 4096);
    let (_evidence, outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    assert!(outcome.exhausted, "state limit should be exhausted");
}

#[test]
fn emission_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let parsed = crate::parse(source, "emit-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream = crate::analysis::facts::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = compile_flow(&query);
    let limits = FlowLimits::test_new(65_536, 262_144, 0, 4096);
    let (_evidence, outcome) = collect_with_limits_test(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    assert!(outcome.exhausted, "emission limit should be exhausted");
}
