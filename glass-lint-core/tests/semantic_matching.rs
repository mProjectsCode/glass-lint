//! Semantic matching coverage for provenance, aliases, and value flow.
//!
//! These cases exercise patterns found in production JavaScript while requiring
//! the matcher to prove each match without falling back to name-only matching.

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    rules::{
        CallMatcher, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher,
        MemberCallMatcher, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        Severity, ValueMatcher,
    },
};

/// Execute one matcher through a fresh strict catalog and return its count.
fn findings(source: &str, matcher: Matcher) -> usize {
    let rule = Rule::builder("semantic.match")
        .label("semantic matcher")
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(matcher)
        .build()
        .unwrap();
    let environment = test_environment();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet(source, "semantic-matching.js")
        .unwrap()
        .files[0]
        .findings
        .len()
}

/// Supply only the provider roots needed by the semantic fixtures.
fn test_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "app", "client", "document", "fetch", "host", "require", "vault",
        ])
        .unwrap();
    environment
}

/// Assert the exact match count for a provenance or value-flow scenario.
fn assert_matches(source: &str, matcher: Matcher, expected: usize) {
    assert_eq!(findings(source, matcher), expected, "{source}");
}

#[test]
fn follows_default_import_namespace_members_through_aliases() {
    assert_matches(
        "import sdk from 'sdk'; const send = sdk.send; send('/x');",
        Matcher::module_member_call("sdk", "send"),
        1,
    );
}

#[test]
fn follows_destructured_esm_namespace_exports() {
    assert_matches(
        "import * as sdk from 'sdk'; const { send } = sdk; send('/x');",
        Matcher::module_call("sdk", "send"),
        1,
    );
}

#[test]
fn follows_destructured_esm_namespace_export_renames() {
    assert_matches(
        "import * as sdk from 'sdk'; const { send: dispatch } = sdk; dispatch('/x');",
        Matcher::module_call("sdk", "send"),
        1,
    );
}

#[test]
fn follows_interop_members_extracted_before_the_call() {
    assert_matches(
        "const send = __toESM(require('sdk')).send; send('/x');",
        Matcher::module_call("sdk", "send"),
        1,
    );
}

#[test]
fn preserves_module_provenance_through_sequence_calls() {
    assert_matches(
        "const sdk = require('sdk'); (0, sdk.send)('/x');",
        Matcher::module_call("sdk", "send"),
        1,
    );
}

#[test]
fn preserves_module_provenance_through_bound_exports() {
    assert_matches(
        "const send = require('sdk').send.bind(null); send('/x');",
        Matcher::module_call("sdk", "send"),
        1,
    );
}

#[test]
fn follows_destructured_rooted_members() {
    assert_matches(
        "const { read } = host.files; read('x');",
        Matcher::rooted_member_call("host.files.read"),
        1,
    );
}

#[test]
fn follows_renamed_destructured_rooted_members() {
    assert_matches(
        "const { read: load } = host.files; load('x');",
        Matcher::rooted_member_call("host.files.read"),
        1,
    );
}

#[test]
fn follows_nested_destructured_rooted_members() {
    assert_matches(
        "const { files: { read } } = host; read('x');",
        Matcher::rooted_member_call("host.files.read"),
        1,
    );
}

#[test]
fn follows_rooted_members_called_via_sequence_expressions() {
    assert_matches(
        "(0, app.commands.execute)('open');",
        MemberCallMatcher::rooted_chain("app.commands.execute")
            .arg_string(0, ["open"])
            .into(),
        1,
    );
}

#[test]
fn follows_bound_rooted_members_and_their_arguments() {
    assert_matches(
        "const open = app.open.bind(app); open(vault.file);",
        MemberCallMatcher::rooted_chain("app.open")
            .arg_rooted_exprs(0, ["vault.file"])
            .into(),
        1,
    );
}

#[test]
fn preserves_bound_rooted_expression_arguments() {
    assert_matches(
        "const open = app.open.bind(app, vault.file); open(actual);",
        MemberCallMatcher::rooted_chain("app.open")
            .arg_rooted_exprs(0, ["vault.file"])
            .into(),
        1,
    );
}

#[test]
fn prepends_static_bound_arguments_before_call_arguments() {
    assert_matches(
        "const request = fetch.bind(null, '/bound'); request('/actual');",
        CallMatcher::global("fetch")
            .arg_string(0, ["/bound"])
            .into(),
        1,
    );
    assert_matches(
        "const request = fetch.bind(null, '/bound'); request('/actual');",
        CallMatcher::global("fetch")
            .arg_string(0, ["/actual"])
            .into(),
        0,
    );
    assert_matches(
        "const send = require('sdk').send.bind(null, '/bound'); send('/actual');",
        CallMatcher::module_export("sdk", "send")
            .arg_string(0, ["/bound"])
            .into(),
        1,
    );
}

#[test]
fn resolves_static_template_literals_without_substitutions() {
    assert_matches(
        "const url = `/remote`; fetch(url);",
        Matcher::call(glass_lint_core::rules::CallMatcher::global("fetch").static_string_arg(0)),
        1,
    );
}

#[test]
fn resolves_constant_template_literal_substitutions() {
    assert_matches(
        "const segment = 'remote'; const url = `/${segment}`; fetch(url);",
        Matcher::call(glass_lint_core::rules::CallMatcher::global("fetch").static_string_arg(0)),
        1,
    );
}

#[test]
fn resolves_static_array_property_names_through_constant_indexes() {
    assert_matches(
        "const names = ['read']; const index = 0; host.files[names[index]]('x');",
        Matcher::rooted_member_call("host.files.read"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_immediately_invoked_arrows() {
    assert_matches(
        "((callback) => callback('/x'))(fetch);",
        Matcher::global_call("fetch"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_immediately_invoked_functions() {
    assert_matches(
        "(function(callback) { callback('/x'); })(fetch);",
        Matcher::global_call("fetch"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_array_iteration() {
    assert_matches(
        "[fetch].forEach(callback => callback('/x'));",
        Matcher::global_call("fetch"),
        1,
    );
}

#[test]
fn joins_matching_values_from_finite_array_callbacks() {
    assert_matches(
        "[fetch, fetch].forEach(callback => callback('/x'));",
        Matcher::global_call("fetch"),
        1,
    );
    assert_matches(
        "[fetch, local].forEach(callback => callback('/x'));",
        Matcher::global_call("fetch"),
        0,
    );
}

#[test]
fn tracks_global_callbacks_through_promise_handlers() {
    assert_matches(
        "Promise.resolve(fetch).then(callback => callback('/x'));",
        Matcher::global_call("fetch"),
        1,
    );
}

#[test]
fn tracks_rooted_arguments_through_destructured_parameters() {
    assert_matches(
        "function open({ file }) { app.open(file); } open({ file: vault.file });",
        MemberCallMatcher::rooted_chain("app.open")
            .arg_rooted_exprs(0, ["vault.file"])
            .into(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_const_spreads() {
    assert_matches(
        "const base = { url: '/x' }; const options = { ...base, method: 'GET' }; client.request(options);",
        MemberCallMatcher::rooted_chain("client.request")
            .arg_object_keys(0, ["url", "method"])
            .into(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_object_assign() {
    assert_matches(
        "const options = Object.assign({}, { url: '/x', method: 'GET' }); client.request(options);",
        MemberCallMatcher::rooted_chain("client.request")
            .arg_object_keys(0, ["url", "method"])
            .into(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_member_function_aliases() {
    assert_matches(
        "const request = client.request; request({ url: '/x', method: 'GET' });",
        MemberCallMatcher::rooted_chain("client.request")
            .arg_object_keys(0, ["url", "method"])
            .into(),
        1,
    );
}

/// Build the source/configuration/sink flow used by flow-provenance tests.
fn script_insertion_matcher() -> Matcher {
    Matcher::flow(
        ObjectFlowMatcher::builder("script insertion")
            .source(ObjectSourceMatcher::returned_by(
                MemberCallMatcher::rooted("document.createElement")
                    .arg(0, ValueMatcher::static_string().equals("script")),
            ))
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "src",
                ValueMatcher::any_value(),
            )))
            .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                MemberCallMatcher::rooted("document.head.appendChild"),
                0,
            )]))
            .build()
            .unwrap(),
    )
}

#[test]
fn tracks_flow_configuration_through_a_source_alias() {
    assert_matches(
        "const script = document.createElement('script'); const alias = script; alias.src = url; document.head.appendChild(script);",
        script_insertion_matcher(),
        1,
    );
}

#[test]
fn tracks_flow_configuration_through_static_computed_properties() {
    assert_matches(
        "const script = document.createElement('script'); script['src'] = url; document.head.appendChild(script);",
        script_insertion_matcher(),
        1,
    );
}

#[test]
fn tracks_flow_sinks_through_rooted_member_aliases() {
    assert_matches(
        "const append = document.head.appendChild; const script = document.createElement('script'); script.src = url; append(script);",
        script_insertion_matcher(),
        1,
    );
}

#[test]
fn tracks_flow_sinks_through_optional_chains() {
    assert_matches(
        "const script = document.createElement('script'); script.src = url; document.head?.appendChild?.(script);",
        script_insertion_matcher(),
        1,
    );
}
