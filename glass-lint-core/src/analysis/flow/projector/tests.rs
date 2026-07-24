use super::*;
use crate::{
    analysis::resolution::Resolver,
    api::rule::{
        FlowCompletion, FlowCondition, FlowSinkMatcher, ObjectEventMatcher, ObjectFlowMatcher,
        ObjectSourceMatcher, ValueMatcher,
    },
};

fn collect_source(source: &str, flow: &ObjectFlowMatcher) -> Vec<Vec<ClassificationEvidence>> {
    let parsed = crate::parse(source, "fact-flow.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = CompiledObjectFlow::from_matcher(flow);
    let (evidence, _outcome) = collect_with_limits(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        FlowLimits::from_flow_operations(262_144),
    );
    evidence
}

fn script_flow() -> ObjectFlowMatcher {
    ObjectFlowMatcher::builder("script insertion")
        .source(
            ObjectSourceMatcher::returned_by("document.createElement")
                .arg(0, ValueMatcher::static_string().equals("script")),
        )
        .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
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
fn member_call_configuration_stays_with_its_receiver() {
    let flow = ObjectFlowMatcher::builder("configured script")
        .source(
            ObjectSourceMatcher::returned_by("document.createElement")
                .arg(0, ValueMatcher::static_string().equals("script")),
        )
        .configured_by(FlowCondition::event(
            ObjectEventMatcher::member_call("configure")
                .arg(0, ValueMatcher::static_string().equals("yes")),
        ))
        .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
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
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn one_arm_requirement_does_not_leak_after_join() {
    let evidence = collect_source(
        "const script = document.createElement('script'); if (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn zero_iteration_loops_do_not_make_body_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); while (ready) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
}

#[test]
fn do_while_body_configuration_is_reachable_after_loop() {
    let evidence = collect_source(
        "const script = document.createElement('script'); do { script.src = url; } while (ready); document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
}

#[test]
fn catch_only_configuration_does_not_become_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); try { work(); } catch (error) { script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert!(evidence[0].is_empty());
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
    assert!(evidence[0].is_empty());
}

#[test]
fn default_case_can_make_configuration_definite() {
    let evidence = collect_source(
        "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; default: script.src = url; } document.head.appendChild(script);",
        &script_flow(),
    );
    assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
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
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
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
    let flow = CompiledObjectFlow::from_matcher(&script_flow());
    let (evidence, _outcome) = collect_with_limits(
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
    let flow = ObjectFlowMatcher::builder("configured input")
        .source(
            ObjectSourceMatcher::returned_by("document.createElement")
                .arg(0, ValueMatcher::static_string().equals("input")),
        )
        .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
            "type",
            ValueMatcher::static_string().equals("file"),
        )))
        .complete_at(FlowCompletion::configuration())
        .build()
        .unwrap();
    let source = "const input = document.createElement('input'); input.type = 'file';";
    let parsed =
        crate::parse(source, "flow-requirement-location.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let configuration = stream
        .facts()
        .iter()
        .find_map(|fact| {
            matches!(fact.payload, FactPayload::PropertyWrite { .. })
                .then_some((fact.id, fact.span))
        })
        .expect("configuration write should be present");
    let flow = CompiledObjectFlow::from_matcher(&flow);
    let (evidence, _outcome) = collect_with_limits(
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
    let flow = script_flow();
    let source = "const a = document.createElement('script'); const b = document.createElement('script');";
    let parsed = crate::parse(source, "obj-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = CompiledObjectFlow::from_matcher(&flow);
    let limits = FlowLimits::test_new(1, 262_144, 65_536, 4096);
    let (evidence, outcome) = collect_with_limits(
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
    let flow = script_flow();
    let source = "const a = document.createElement('script'); const b = document.createElement('script');";
    let parsed = crate::parse(source, "mut-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = CompiledObjectFlow::from_matcher(&flow);
    let limits = FlowLimits::test_new(65_536, 262_144, 65_536, 1);
    let (_evidence, outcome) = collect_with_limits(
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
    let flow = script_flow();
    let source = "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let parsed = crate::parse(source, "state-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = CompiledObjectFlow::from_matcher(&flow);
    let limits = FlowLimits::test_new(65_536, 0, 65_536, 4096);
    let (_evidence, outcome) = collect_with_limits(
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
    let flow = script_flow();
    let source = "const a = document.createElement('script'); a.src = url; document.head.appendChild(a);";
    let parsed = crate::parse(source, "emit-limit.js").expect("source should parse");
    let mut resolver = Resolver::collect(&parsed.program, source);
    let stream =
        crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
    let effects = FunctionEffects::collect(&stream, usize::MAX);
    let flow = CompiledObjectFlow::from_matcher(&flow);
    let limits = FlowLimits::test_new(65_536, 262_144, 0, 4096);
    let (_evidence, outcome) = collect_with_limits(
        &stream,
        &effects,
        &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
        1,
        limits,
    );
    assert!(outcome.exhausted, "emission limit should be exhausted");
}
