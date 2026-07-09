//! Regression coverage for the precision failures identified in `review.md`.
//!
//! Every case uses the public linting API so it covers parsing, scope
//! collection, semantic resolution, and matcher execution together.

use glass_lint_core::{
    Linter, RuleCatalog,
    rules::{Builder, Confidence, Matcher, Rule, Severity},
};

fn rule(id: &str) -> Builder {
    Rule::builder(id)
        .label(id)
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

fn assert_count(source: &str, rule: Rule, expected: usize) {
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let report = Linter::new(catalog).lint(source, "review-regression.js");
    assert!(report.parse_diagnostics.is_empty(), "{source}");
    assert_eq!(report.findings.len(), expected, "{source}");
}

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
