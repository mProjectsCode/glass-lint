use super::*;

#[test]
fn negative_alias_to_sink_not_configured() {
    // Object aliased and sunk but never configured (no requirement).
    let flow = LifecycleQuery::catalog_builder("alias-sink")
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
        .unwrap();

    let rule = rule("neg.alias-sink")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // Aliased and sunk, but not configured
    let report = single_lint(
        "const s = document.createElement('script'); const a = s; document.head.appendChild(a);",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "alias-to-sink without configuration must not produce a finding"
    );
}

#[test]
fn negative_requirement_to_sink_disconnected_object() {
    // One object configured, different object sunk.
    let flow = LifecycleQuery::catalog_builder("req-sink")
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
        .unwrap();

    let rule = rule("neg.req-sink")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // s1 is configured, s2 is sunk — different objects
    let report = single_lint(
        "const s1 = document.createElement('script'); s1.src = 'https://evil'; const s2 = document.createElement('script'); document.head.appendChild(s2);",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "configured object and sunk object must be the same tracked object"
    );
}

// ── 9. Completion state ──────────────────────────────────────────────

#[test]
fn baseline_completion_is_complete_for_simple_query() {
    let report = single_lint(
        "fetch('/data');",
        rule("simple-complete")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
    );
    assert_eq!(report.completion(), ReportCompletion::Complete);
}

// ── 10. Finding order is deterministic ────────────────────────────────

#[test]
fn baseline_finding_order_is_deterministic() {
    let report_a = single_lint(
        "fetch('/a'); fetch('/b');",
        rule("order-test")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
    );
    let report_b = single_lint(
        "fetch('/a'); fetch('/b');",
        rule("order-test")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
    );

    let ids_a: Vec<_> = report_a.files()[0]
        .findings()
        .iter()
        .map(|f| f.location().range().start().line())
        .collect();
    let ids_b: Vec<_> = report_b.files()[0]
        .findings()
        .iter()
        .map(|f| f.location().range().start().line())
        .collect();

    // Same source produces same finding order
    assert_eq!(ids_a, ids_b);

    // Two findings, in source order
    assert_eq!(ids_a.len(), 2);
    assert_eq!(ids_a[0], 1);
    assert_eq!(ids_a[1], 1); // same line, different column positions
}

// ── 11. Operation counts are stable ──────────────────────────────────

#[test]
fn baseline_operation_counts_are_stable() {
    let report = single_lint(
        "fetch('/data');",
        rule("ops-stable")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
    );
    let ops = report.operations();
    // Single file → 1 file operation
    assert_eq!(ops.files(), 1);
    assert_eq!(ops.requests(), 0);
    assert_eq!(ops.edges(), 0);
    assert_eq!(ops.exports(), 0);
    assert_eq!(ops.scc_rounds(), 0);
    assert_eq!(ops.effect_projections(), 0);
    assert_eq!(ops.evidence(), 1);
    assert_eq!(ops.max_live_alternatives(), 0);
    assert_eq!(ops.trace_nodes(), 0);
    assert_eq!(ops.trace_heads(), 0);
    assert_eq!(ops.coalescing_comparisons(), 0);
    assert_eq!(ops.fixed_point_iterations(), 0);
    assert_eq!(ops.rendered_traces(), 1);
}

#[test]
fn all_sink_correlation_has_deterministic_bounded_operations() {
    let lifecycle = LifecycleQuery::catalog_builder("two-sinks")
        .source(Ok(
            EventQuery::member_call_rooted("document.createElement").unwrap()
        ))
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::all_sinks([
            LifecycleSink::argument_of_member("document.head.appendChild", 0).unwrap(),
            LifecycleSink::argument_of_member("document.body.appendChild", 0).unwrap(),
        ]))
        .build()
        .unwrap();
    let report = single_lint(
        "const node = document.createElement('script'); node.src = url; document.head.appendChild(node); document.body.appendChild(node);",
        rule("two-sinks")
            .query(QueryDecl::lifecycle(Ok(lifecycle)))
            .build()
            .unwrap(),
    );
    let ops = report.operations();
    assert_eq!(report.files()[0].findings().len(), 1);
    let steps = report.files()[0].findings()[0].evidence().traces()[0].steps();
    assert_eq!(
        steps.len(),
        4,
        "source, configuration, first sink, final sink"
    );
    assert_eq!(ops.evidence(), 4);
    assert!(ops.fixed_point_iterations() <= 16);
    assert!(ops.trace_nodes() <= 16);
}

#[test]
fn duplicate_query_roots_are_deduplicated_before_execution() {
    let query = EventQuery::call_global("fetch").unwrap().into_query();
    let report = single_lint(
        "fetch('/data');",
        rule("duplicate-roots")
            .query(query.clone())
            .query(query)
            .build()
            .unwrap(),
    );
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(report.operations().evidence(), 1);
}
