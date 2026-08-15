use super::*;

#[test]
fn catch_sink_can_consume_a_source_from_before_try() {
    let evidence = collect_source(
        "const script = document.createElement('script'); script.src = url; try { work(); } catch (error) { document.head.appendChild(script); }",
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
fn finally_configuration_is_applied_to_normal_completion() {
    let evidence = collect_source(
        "const script = document.createElement('script'); try { work(); } finally { script.src = url; } document.head.appendChild(script);",
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
fn switch_no_match_path_prevents_case_only_configuration() {
    let evidence = collect_source(
        "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; } document.head.appendChild(script);",
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
fn default_case_can_make_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; default: script.src = url; } document.head.appendChild(script);",
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
fn incompatible_branch_facts_do_not_form_a_flow_witness() {
    let evidence = collect_source(
        "const script = document.createElement('script'); let inserted; if (ready) { script.src = url; inserted = localElement; } else { inserted = script; } document.head.appendChild(inserted);",
        &script_flow(),
    );
    assert_eq!(evidence[0].len(), 0);
}

#[test]
fn source_created_on_one_branch_can_reach_a_possible_sink_after_join() {
    let evidence = collect_source(
        "let script; if (ready) { script = document.createElement('script'); script.src = url; } document.head.appendChild(script);",
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
fn do_while_break_preserves_the_break_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { script.src = url; break; } while (ready); document.head.appendChild(script);",
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
fn finally_configuration_reaches_a_break_exit() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { try { break; } finally { script.src = url; } } while (ready); document.head.appendChild(script);",
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
fn finally_return_does_not_reach_code_after_the_try() {
    let evidence = collect_source(
        "function run() { const script = document.createElement('script'); try { return; } finally { script.src = url; } document.head.appendChild(script); }",
        &script_flow(),
    );
    assert_eq!(evidence[0].len(), 0);
}

#[test]
fn destructuring_assignment_invalidates_the_written_alias() {
    let evidence = collect_source(
        "let script = document.createElement('script'); script.src = url; ({ script } = replacement); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].len(), 0);
}

#[test]
fn rebinding_one_alias_does_not_kill_the_shared_object() {
    let evidence = collect_source(
        "let first = document.createElement('script'); const alias = first; first = replacement; alias.src = url; document.head.appendChild(alias);",
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
fn flow_evidence_is_anchored_at_the_sink_event() {
    let source = "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);";
    let stream = crate::analysis::facts::build_test_facts(source, "flow-location.js");
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let sink_span = stream
        .facts()
        .iter()
        .find_map(|fact| match &fact.payload {
            FactPayload::Call(call)
                if call.syntactic_path().is_some_and(|chain| {
                    stream
                        .names()
                        .resolve_path(chain)
                        .is_some_and(|s| s.eq_chain("document.head.appendChild"))
                }) =>
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
    assert_eq!(evidence[0][0].occurrences()[0].span(), sink_span);
}

#[test]
fn requirement_only_evidence_is_anchored_at_the_configuration_event() {
    let flow = LifecycleQuery::catalog_builder("configured input")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(
                    0,
                    ValueMatcher::static_string().try_equals("input").unwrap(),
                ),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::static_string().try_equals("file").unwrap(),
        )))
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    let source = "const input = document.createElement('input'); input.type = 'file';";
    let stream = crate::analysis::facts::build_test_facts(source, "flow-requirement-location.js");
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
    assert_eq!(evidence[0][0].occurrences()[0].span(), configuration.1);
    assert_eq!(
        evidence[0][0].occurrences()[0].fact(),
        Some(configuration.0.raw_for_test())
    );
}

#[test]
fn object_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source = "const a = document.createElement('script'); a.src = url; document.head.appendChild(a); const b = document.createElement('script');";
    let stream = crate::analysis::facts::build_test_facts(source, "obj-limit.js");
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
    assert!(outcome.is_exhausted(), "object limit should be exhausted");
    assert_eq!(evidence[0].len(), 1);
    assert_eq!(
        evidence[0][0].certainty(),
        crate::project::MatchCertainty::Possible
    );
}

#[test]
fn mutation_log_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); const b = document.createElement('script');";
    let stream = crate::analysis::facts::build_test_facts(source, "mut-limit.js");
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
    assert!(
        outcome.is_exhausted(),
        "mutation log limit should be exhausted"
    );
}

#[test]
fn state_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let stream = crate::analysis::facts::build_test_facts(source, "state-limit.js");
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
    assert!(outcome.is_exhausted(), "state limit should be exhausted");
}

#[test]
fn emission_limit_exhaustion_returns_exhausted_outcome() {
    let query = script_flow();
    let source =
        "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let stream = crate::analysis::facts::build_test_facts(source, "emit-limit.js");
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
    assert!(outcome.is_exhausted(), "emission limit should be exhausted");
}
