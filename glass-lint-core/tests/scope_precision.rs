//! Scope and provenance precision coverage.
//!
//! Every case uses the public linting API so it covers parsing, scope
//! collection, semantic resolution, and matcher execution together.

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    rules::{Matcher, Rule},
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
    assert!(!report.files[0].has_parse_diagnostics(), "{source}");
    assert_eq!(report.files[0].findings.len(), expected, "{source}");
}

/// Create the rooted alias rule shared by lexical-scope cases.
fn rooted_read_rule() -> Rule {
    rule("rooted-read")
        .matcher(Matcher::rooted_member_call("host.files.read"))
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
        .matcher(Matcher::import("@codemirror/state"))
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
        .matcher(Matcher::global_call("fetch"))
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
        .matcher(Matcher::rooted_member_call("host.cache.read"))
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
        .matcher(Matcher::rooted_member_call("host.open.execute"))
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
        .matcher(Matcher::module_call("sdk", "send"))
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
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    assert_count("let value = dynamicThing(); value('/x');", rule, 0);
}
