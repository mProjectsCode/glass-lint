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
    assert_eq!(report.files()[0].findings().len(), expected, "{source}");
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

#[test]
fn conditional_assignment_never_falls_back_to_an_older_identity() {
    let rule = rooted_read_rule();
    assert_count(
        "let api = host.files; if (flag) api = local.files; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = local.files; if (flag) api = host.files; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = host.files; if (flag) api = host.files; else api = host.files; api.read();",
        rule,
        1,
    );
}

#[test]
fn assignment_provenance_isolated_across_control_flow_paths() {
    let rule = rooted_read_rule();
    assert_count(
        "let api = local.files; if (flag) api = host.files; else api = api; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = local.files; if (outer) { if (inner) api = host.files; } else api = api; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = host.files; while (flag) api = local.files; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = local.files; while (flag) api = host.files; api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = local.files; switch (kind) { case 0: api = host.files; break; case 1: api = api; break; } api.read();",
        rule,
        0,
    );
    assert_count(
        "let api = host.files; while (flag) { api = local.files; break; } api.read();",
        rooted_read_rule(),
        0,
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

#[test]
fn exceptional_edges_join_try_and_catch_assignments() {
    let rule = rooted_read_rule();
    assert_count(
        "let api = local.files; try { api = host.files; } catch { api = api; } api.read();",
        rule.clone(),
        0,
    );
    assert_count(
        "let api = host.files; try { api = host.files; } catch { api = host.files; } api.read();",
        rule.clone(),
        1,
    );
    assert_count(
        "let api = host.files; try { api = local.files; } catch {} api.read();",
        rule,
        0,
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
