//! Deterministic regression baselines for representative query shapes.
//!
//! Each test asserts stable operation fields (finding count, certainty,
//! evidence trace count, operation counts, completion state) rather than
//! opaque report snapshots.  When operation counts or findings change
//! intentionally, update the expected values here.
//!
//! See `reports/QUERY_MIGRATION_BASELINE.md` for the explanatory report.

use glass_lint_core::{
    MatchCertainty,
    project::ReportCompletion,
    rules::{
        EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
        LifecycleSink, QueryDecl, Rule, ValueMatcher,
    },
};

use crate::support::{self, rule};

// ── Helpers ────────────────────────────────────────────────────────────

fn single_lint(source: &str, rule: Rule) -> glass_lint_core::project::AnalysisReport {
    support::lint_report(source, rule)
}

// ── 1. Simple indexed query (global call) ─────────────────────────────

#[test]
fn baseline_simple_global_call() {
    let report = single_lint(
        "fetch('/data');",
        rule("fetch")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
    );
    let findings = &report.files()[0].findings();

    // One finding, definite, one evidence trace
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);
    assert_eq!(findings[0].evidence().traces().len(), 1);

    // Completion complete (single file, no budget issues)
    assert_eq!(report.completion(), ReportCompletion::Complete);
}

// ── 2. Constrained call ───────────────────────────────────────────────

#[test]
fn baseline_constrained_call() {
    let report = single_lint(
        "fetch('/api/data');",
        rule("fetch-constrained")
            .query(
                EventQuery::call_global("fetch")
                    .unwrap()
                    .with_arg(
                        0,
                        ValueMatcher::static_string()
                            .try_equals("/api/data")
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
    );
    let findings = &report.files()[0].findings();

    // Static value matches → one definite finding
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);

    // Mis-matched value produces no finding
    let no_match = single_lint(
        "fetch('/other');",
        rule("fetch-constrained-no")
            .query(
                EventQuery::call_global("fetch")
                    .unwrap()
                    .with_arg(
                        0,
                        ValueMatcher::static_string()
                            .try_equals("/api/data")
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query(),
            )
            .build()
            .unwrap(),
    );
    assert_eq!(no_match.files()[0].findings().len(), 0);
}

// ── 3. Returned-object query ──────────────────────────────────────────

#[test]
fn baseline_returned_object() {
    let report = single_lint(
        "const el = document.createElement('script'); el.appendChild(null);",
        rule("returned")
            .query(QueryDecl::member_call_returned(
                "document.createElement",
                "appendChild",
            ))
            .build()
            .unwrap(),
    );
    let findings = &report.files()[0].findings();

    // Returned-object correlation produces one definite finding
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);
    assert_eq!(findings[0].evidence().traces().len(), 1);
}

// ── 4. Constructed-instance query ────────────────────────────────────

#[test]
fn baseline_constructed_instance() {
    // Instance correlation requires module resolution; in a snippet
    // this test verifies the physical routing rather than a live match.
    // The physical planner produces an InstanceSubject root.
    let report = single_lint(
        "const c = new Client(); c.send();",
        rule("instance")
            .query(QueryDecl::member_call_instance("pkg", "Client", "send"))
            .build()
            .unwrap(),
    );

    // Without module resolution for pkg.Client, no definite finding.
    let findings = &report.files()[0].findings();
    assert_eq!(findings.len(), 0);
}

// ── 5. Local lifecycle ────────────────────────────────────────────────

#[test]
fn baseline_local_lifecycle() {
    let flow = LifecycleQuery::catalog_builder("script-insert")
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

    let rule = rule("lifecycle.local")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    let report = single_lint(
        "const s = document.createElement('script'); s.src = 'https://evil'; document.head.appendChild(s);",
        rule,
    );
    let findings = &report.files()[0].findings();

    // Complete source→configuration→sink lifecycle
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);

    // Flow projection produces operation counts
    let ops = report.operations();
    assert_eq!(ops.effect_projections(), 1);
    assert_eq!(ops.evidence(), 3);
    assert_eq!(ops.max_live_alternatives(), 1);
    assert_eq!(ops.trace_nodes(), 3);
    assert_eq!(ops.trace_heads(), 1);
    assert_eq!(ops.coalescing_comparisons(), 0);
    assert_eq!(ops.fixed_point_iterations(), 0);
    assert_eq!(ops.rendered_traces(), 1);
}

// ── 6. Within-function lifecycle (same as cross-call in snippet mode) ──

#[test]
fn baseline_within_function_lifecycle() {
    // Local flow projection traces an object through source, configuration,
    // and sink when all occur in the same scope.
    let flow = LifecycleQuery::catalog_builder("script-local-flow")
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

    let rule = rule("lifecycle.within-fn")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // All in one scope: full source → configuration → sink chain
    let report = single_lint(
        "const s = document.createElement('script'); s.src = 'https://evil'; document.head.appendChild(s);",
        rule,
    );
    let findings = &report.files()[0].findings();

    // Local flow resolves the complete lifecycle
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);
}

// ── 7. Project module identity ───────────────────────────────────────

#[test]
fn baseline_project_module_identity() {
    let report = single_lint(
        "import { readFile } from 'fs'; readFile('/etc/passwd');",
        rule("module-call")
            .query(EventQuery::call_module("fs", "readFile"))
            .build()
            .unwrap(),
    );
    let findings = &report.files()[0].findings();

    // Module identity resolved through project overlay
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].certainty(), MatchCertainty::Definite);
}

// ── 8. Flow join negatives ───────────────────────────────────────────

#[test]
fn negative_source_to_alias_no_sink() {
    // Source object is aliased but never flows to a sink.
    let flow = LifecycleQuery::catalog_builder("source-alias")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(0, ValueMatcher::static_string().try_equals("div").unwrap()),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "textContent",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("document.body.appendChild", 0),
        ]))
        .build()
        .unwrap();

    let rule = rule("neg.source-alias")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // Source created and aliased, but never reaches sink
    let report = single_lint(
        "const d = document.createElement('div'); const e = d; e.textContent = 'hello';",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "source-to-alias without sink must not produce a finding"
    );
}

#[test]
fn negative_source_to_requirement_no_sink() {
    // Source configured but never sunk.
    let flow = LifecycleQuery::catalog_builder("source-req")
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

    let rule = rule("neg.source-req")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    let report = single_lint(
        "const s = document.createElement('script'); s.src = 'https://evil';",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "source-to-requirement without sink must not produce a finding"
    );
}

#[test]
fn negative_disconnected_source_and_sink() {
    // Source and sink exist but are not connected by flow:
    // createElement('div') does not match the "script" arg filter.
    let flow = LifecycleQuery::catalog_builder("disconnected")
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

    let rule = rule("neg.disconnected")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // Source is createElement('div') which does not match the "script" arg filter
    let report = single_lint(
        "const s = document.createElement('div'); s.src = 'https://evil'; document.head.appendChild(s);",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "source with non-matching arg must not produce a finding even when sink matches"
    );
}

#[test]
fn negative_source_wrong_arg_no_match() {
    // Source with non-matching argument should not trigger.
    let flow = LifecycleQuery::catalog_builder("source-arg")
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

    let rule = rule("neg.source-arg")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    let report = single_lint(
        "const s = document.createElement('div'); s.src = 'https://evil'; document.head.appendChild(s);",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "source with non-matching argument must not produce a finding"
    );
}

#[test]
fn negative_escaped_object_no_lifecycle() {
    // Object escapes tracked scope (returned to unknown caller).
    let flow = LifecycleQuery::catalog_builder("escaped")
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

    let rule = rule("neg.escaped")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // Object is returned — the caller could do anything with it.
    let report = single_lint(
        "function make() { const s = document.createElement('script'); s.src = 'https://evil'; return s; }",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "escaped object must not produce a finding without sink"
    );
}

#[test]
fn negative_alias_to_requirement_no_sink() {
    // Object aliased and configured but never reaches a sink.
    let flow = LifecycleQuery::catalog_builder("alias-req")
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

    let rule = rule("neg.alias-req")
        .query(QueryDecl::lifecycle(Ok(flow)))
        .build()
        .unwrap();

    // Aliased and configured, but no sink
    let report = single_lint(
        "const s = document.createElement('script'); const a = s; a.src = 'https://evil';",
        rule,
    );
    assert_eq!(
        report.files()[0].findings().len(),
        0,
        "alias-to-requirement without sink must not produce a finding"
    );
}

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
