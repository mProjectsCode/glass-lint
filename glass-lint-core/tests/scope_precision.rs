//! Scope and provenance precision coverage.
//!
//! Every case uses the public linting API so it covers parsing, scope
//! collection, semantic resolution, and matcher execution together.

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    rules::{MatcherDecl, Rule},
};

#[path = "support/mod.rs"]
mod support;

use support::rule;

/// Assert exact findings and reject parser diagnostics before checking
/// semantics.
fn assert_count(source: &str, rule: Rule, expected: usize) {
    let mut environment = Environment::default();
    environment
        .add_globals(["fetch", "host", "require"])
        .unwrap();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet(source, "scope-precision.js")
        .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics(), "{source}");
    let count = report.files()[0].findings().len();
    if count != expected {
        eprintln!("UNEXPECTED FINDING COUNT for source: {source}");
        for f in report.files()[0].findings() {
            eprintln!("  rule={:?}, certainty={:?}", f.rule_id(), f.certainty());
        }
    }
    assert_eq!(count, expected, "{source}");
}

/// Create the rooted alias rule shared by lexical-scope cases.
fn rooted_read_rule() -> Rule {
    rule("rooted-read")
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("host.files.read")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}

#[test]
fn loop_header_lexical_bindings_do_not_escape_or_shadow_outer_aliases() {
    assert_count(
        "const api = host.files; for (let api of [local.files]) api.read(); api.read();",
        rooted_read_rule(),
        1,
    );
    assert_count(
        "for (let api = host.files; false;) {} api.read();",
        rooted_read_rule(),
        0,
    );
    assert_count(
        "for (let api in { value: 1 }) api.read(); api.read();",
        rooted_read_rule(),
        0,
    );
}

#[test]
fn loop_header_var_bindings_remain_function_scoped() {
    assert_count(
        "for (var api = host.files; false;) {} api.read();",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn switch_lexical_bindings_do_not_escape_or_shadow_outer_aliases() {
    assert_count(
        "const api = host.files; switch (kind) { case 'local': let api = local.files; api.read(); break; } api.read();",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn property_aliases_follow_the_same_receiver_binding_and_version() {
    assert_count(
        "const table = {}; table.cache = host.files; table.cache.read(); function unrelated(table) { table.cache.read(); } { const table = {}; table.cache.read(); }",
        rooted_read_rule(),
        1,
    );
    assert_count(
        "let table = {}; table.cache = host.files; table = {}; table.cache.read();",
        rooted_read_rule(),
        0,
    );
    assert_count(
        "const table = {}; table.cache = host.files; function nested() { table.cache.read(); }",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn import_matchers_reject_shadowed_commonjs_loaders() {
    let require_rule = rule("import")
        .declaration(
            MatcherDecl::builder()
                .import_exact("@codemirror/state")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(
        "function require(name) { return { anything() {} }; } require('@codemirror/state');",
        require_rule.clone(),
        0,
    );
    assert_count(
        "function load(require) { require('@codemirror/state'); }",
        require_rule.clone(),
        0,
    );
    assert_count(
        "const require = localRequire; require('@codemirror/state');",
        require_rule.clone(),
        0,
    );
    assert_count("require('@codemirror/state');", require_rule, 1);
}

#[test]
fn dynamic_scopes_fail_closed_without_affecting_ordinary_globals() {
    let fetch_rule = rule("fetch")
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(
        "with ({ fetch() {} }) { fetch('/local'); } fetch('/global');",
        fetch_rule.clone(),
        1,
    );
    assert_count(
        "fetch('/before'); eval('var fetch = () => {}'); fetch('/after');",
        fetch_rule.clone(),
        1,
    );
    assert_count(
        "function eval() {} eval('not dynamic'); fetch('/global');",
        fetch_rule.clone(),
        1,
    );
    assert_count("fetch('/global');", fetch_rule, 1);
}

#[test]
fn alias_classifier_handles_reassignment_to_a_rooted_member() {
    // The classifier must consume the same cached subresults for the
    // declaration and the later reassignment. A bare call should remain
    // local, but an assignment to a host-returned object must propagate
    // the rooted identity to the use position.
    let rule = rule("reassign-rooted")
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("host.cache.read")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(
        "let api = host.files; api = host.cache; api.read();",
        rule,
        1,
    );
}

#[test]
fn precedence_promotes_bound_callable_over_later_aliased_reassignments() {
    // A `host.open.bind(null, ...)` is a bound callable; reassigning the
    // variable to the same expression must keep the bound callable
    // provenance as the higher-priority fact at the call site.
    let rule = rule("bound-callable")
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("host.open.execute")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(
        "let open = host.open.bind(null, host.file); open = host.open.bind(null, host.file); open.execute();",
        rule,
        1,
    );
}

#[test]
fn destructured_require_aliases_record_named_module_exports() {
    // A destructured `require` call must still flow through the
    // classifier as a `Require` so the downstream collect step records
    // each named property as a `ModuleExport` binding.
    let rule = rule("sdk-send")
        .declaration(
            MatcherDecl::builder()
                .call_module("sdk", "send")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count("const { send } = require('sdk'); send('/x');", rule, 1);
}

#[test]
fn dynamic_call_value_does_not_promote_to_a_strict_provenance() {
    // A bare dynamic call must not become a callable, module, or static
    // provenance. The classifier falls back to a returned-object or local
    // binding, which keeps the matcher from observing a strict fact.
    let rule = rule("strict-fetch")
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count("let value = dynamicThing(); value('/x');", rule, 0);
}

/// Semantic matrix for possible-path certainty.
///
/// - `Possible` when at least one reaching path matches, but not all.
/// - `Definite` when every reaching path matches.
#[test]
fn conditional_assignment_preserves_each_feasible_identity() {
    let rule = rooted_read_rule();
    // Possible: host on the incoming/false path.
    assert_count(
        "let api = host.files; if (flag) api = local.files; api.read();",
        rule.clone(),
        1,
    );
    // Possible: host on the true path.
    assert_count(
        "let api = local.files; if (flag) api = host.files; api.read();",
        rule.clone(),
        1,
    );
    // Definite: every reaching path has the same identity.
    assert_count(
        "let api = host.files; if (flag) api = host.files; else api = host.files; api.read();",
        rule,
        1,
    );
}

/// Possible finding: host identity on the incoming (false) path only.
#[test]
fn possible_finding_host_on_false_path() {
    assert_count(
        "let api = host.files; if (flag) api = local.files; api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Possible finding: host identity on the true path only.
#[test]
fn possible_finding_host_on_true_path() {
    assert_count(
        "let api = local.files; if (flag) api = host.files; api.read();",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn branch_local_use_does_not_fall_back_to_the_incoming_alias() {
    assert_count(
        "let api = host.files; if (flag) { api = local.files; api.read(); }",
        rooted_read_rule(),
        0,
    );
    assert_count(
        "let api = host.files; if (flag) api = local.files; else api = local.files; api.read();",
        rooted_read_rule(),
        0,
    );
}

/// Definite: every reaching modeled path has the host identity.
#[test]
fn definite_all_paths_match() {
    assert_count(
        "let api = host.files; if (flag) api = host.files; else api = host.files; api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Limit-exhaustion: when analysis limits prevent a complete result,
/// the certainty must never be Definite even if all retained alternatives
/// happen to match. These tests will be enabled once bounded alternative
/// environments and their explicit limits are implemented.
///
/// Deeply nested branches should not cause unbounded analysis and must
/// not produce a Definite finding when the alternative cap is reached.
#[test]
#[ignore = "alternative limits not yet implemented"]
fn deep_nesting_under_limit_produces_possible_not_definite() {
    // A deeply nested if/else with matching facts, but the alternative
    // count exceeds the configured limit. The retained match should be
    // Possible, not Definite.
    use std::fmt::Write;
    let mut source = String::from("let api = host.files;\n");
    for i in 0..100 {
        let _ = writeln!(
            source,
            "if (flag{i}) api = host.files; else {{ api = local.files; return; }}"
        );
    }
    source.push_str("api.read();");
    assert_count(&source, rooted_read_rule(), 1);
}

/// Many distinct trace alternatives at a single occurrence must be capped.
#[test]
#[ignore = "trace limits not yet implemented"]
fn many_distinct_traces_are_capped_and_marked_truncated() {
    // When the same finding could be reached through many different
    // trace paths, the trace count must be bounded and the finding
    // must report truncation.
    assert_count(
        "let api = host.files; \
         if (a) api = host.files; if (b) api = host.files; \
         if (c) api = host.files; if (d) api = host.files; \
         api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Exhausted alternative budget must downgrade Definite to Possible.
#[test]
#[ignore = "alternative limits not yet implemented"]
fn exhausted_alternative_budget_prevents_definite() {
    // Every retained alternative matches, but some were dropped due to
    // the budget. The finding must be Possible, not Definite.
    assert_count(
        "let api = host.files; \
         if (a) api = host.files; if (b) api = host.files; \
         if (c) api = local; if (d) api = local; \
         api.read();",
        rooted_read_rule(),
        0,
    );
}

/// No finding: neither path has the host identity.
#[test]
fn neither_path_has_identity() {
    assert_count(
        "let api = local.files; if (flag) api = other.files; api.read();",
        rooted_read_rule(),
        0,
    );
}

#[test]
fn assignment_provenance_preserves_alternatives_across_control_flow() {
    let rule = rooted_read_rule();
    // Possible: host.files on the true branch, local.files on the else branch.
    assert_count(
        "let api = local.files; if (flag) api = host.files; else api = api; api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on inner(true,true), local on the others.
    assert_count(
        "let api = local.files; if (outer) { if (inner) api = host.files; } else api = api; api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on the no-entry path, local.files on the body path.
    assert_count(
        "let api = host.files; while (flag) api = local.files; api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on the body path, local.files on the no-entry path.
    assert_count(
        "let api = local.files; while (flag) api = host.files; api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on the case-0 path, local.files on the others.
    assert_count(
        "let api = local.files; switch (kind) { case 0: api = host.files; break; case 1: api = api; break; } api.read();",
        rule,
        1,
    );
    // Possible: host.files on the no-entry path, local.files on the break path.
    assert_count(
        "let api = host.files; while (flag) { api = local.files; break; } api.read();",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn abrupt_branch_exit_does_not_poison_the_reachable_join() {
    assert_count(
        "function run(flag) { let api = host.files; if (flag) { api = local.files; return; } api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// Abrupt exits (return, throw, break, continue) must be excluded from
/// certainty quantification when they do not reach the occurrence.
///
/// Under future possible-path semantics:
/// - Only the non-throwing/non-returning path reaches the sink, so a match on
///   that path is Definite.
#[test]
fn throw_exit_excludes_unreachable_path_from_certainty() {
    assert_count(
        "function run(flag) { let api = host.files; if (flag) { api = local.files; throw new Error(); } api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// Possible: break exits the loop body, leaving only the no-entry path
/// with the host identity.
#[test]
fn loop_break_excludes_nonmatching_body_path() {
    assert_count(
        "let api = host.files; while (flag) { api = local.files; break; } api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Possible: continue skips the rest of the iteration body,
/// but the non-entry path still has the host identity.
#[test]
fn loop_continue_excludes_nonmatching_iteration() {
    assert_count(
        "let api = host.files; let i = 0; while (i < 10) { i++; api = local.files; continue; api.read(); } api.read();",
        rooted_read_rule(),
        1,
    );
}

/// Definite: the only reaching path after return has the identity.
#[test]
fn definite_abrupt_return_excludes_nonmatching_path() {
    assert_count(
        "function run(flag) { let api = local.files; if (flag) api = host.files; else return; api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// Definite: the only reaching path after throw has the identity.
#[test]
fn definite_abrupt_throw_excludes_nonmatching_path() {
    assert_count(
        "function run(flag) { let api = local.files; if (flag) api = host.files; else throw new Error(); api.read(); }",
        rooted_read_rule(),
        1,
    );
}

/// When no abrupt exit removes the conflicting path, the finding is
/// Possible because at least one reaching path matches.
#[test]
fn no_abrupt_exit_produces_possible_finding() {
    assert_count(
        "function run(flag) { let api = host.files; if (flag) api = local.files; api.read(); }",
        rooted_read_rule(),
        1,
    );
}

#[test]
fn exceptional_edges_join_try_and_catch_assignments() {
    let rule = rooted_read_rule();
    // Possible: host.files on the try path, local on the catch path.
    assert_count(
        "let api = local.files; try { api = host.files; } catch { api = api; } api.read();",
        rule.clone(),
        1,
    );
    // Definite: both try and catch have host.files.
    assert_count(
        "let api = host.files; try { api = host.files; } catch { api = host.files; } api.read();",
        rule.clone(),
        1,
    );
    // Possible: host.files on the catch path (unchanged), local on the try path.
    assert_count(
        "let api = host.files; try { api = local.files; } catch {} api.read();",
        rule,
        1,
    );
}

#[test]
fn direct_alias_reassignment_to_local_stays_local() {
    let rule = rule("fetch")
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    assert_count(
        "let reassignedFetch = fetch; reassignedFetch = localFetch; reassignedFetch('/local');",
        rule,
        0,
    );
}
