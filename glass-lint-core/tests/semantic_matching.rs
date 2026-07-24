//! Semantic matching coverage for provenance, aliases, and value flow.
//!
//! These cases exercise patterns found in production JavaScript while requiring
//! the matcher to prove each match without falling back to name-only matching.

use glass_lint_core::rules::{
    ArgumentMatcher, FlowCompletion, FlowCondition, FlowSinkMatcher, MatcherDecl,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ValueMatcher,
};

#[path = "support/mod.rs"]
mod support;

/// Execute one matcher through a fresh strict catalog and return its count.
fn findings(source: &str, decl: MatcherDecl) -> usize {
    let rule = support::rule("semantic.match")
        .declaration(decl)
        .build()
        .unwrap();
    let environment = support::test_environment();
    let catalog = glass_lint_core::RuleCatalog::new("test", vec![rule]).unwrap();
    let (_, _, files, _, _, _) = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
        vec![catalog],
        environment,
    ))
    .unwrap()
    .lint_snippet(source, "semantic-matching.js")
    .unwrap()
    .into_parts();
    files[0].findings().len()
}

/// Assert the exact match count for a provenance or value-flow scenario.
fn assert_matches(source: &str, decl: MatcherDecl, expected: usize) {
    assert_eq!(findings(source, decl), expected, "{source}");
}

#[test]
fn follows_default_import_namespace_members_through_aliases() {
    assert_matches(
        "import sdk from 'sdk'; const send = sdk.send; send('/x');",
        MatcherDecl::builder()
            .member_call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_destructured_esm_namespace_exports() {
    assert_matches(
        "import * as sdk from 'sdk'; const { send } = sdk; send('/x');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_destructured_esm_namespace_export_renames() {
    assert_matches(
        "import * as sdk from 'sdk'; const { send: dispatch } = sdk; dispatch('/x');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_interop_members_extracted_before_the_call() {
    assert_matches(
        "const send = __toESM(require('sdk')).send; send('/x');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn preserves_module_provenance_through_sequence_calls() {
    assert_matches(
        "const sdk = require('sdk'); (0, sdk.send)('/x');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn preserves_module_provenance_through_bound_exports() {
    assert_matches(
        "const send = require('sdk').send.bind(null); send('/x');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_destructured_rooted_members() {
    assert_matches(
        "const { read } = host.files; read('x');",
        MatcherDecl::builder()
            .member_call_rooted("host.files.read")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_renamed_destructured_rooted_members() {
    assert_matches(
        "const { read: load } = host.files; load('x');",
        MatcherDecl::builder()
            .member_call_rooted("host.files.read")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_nested_destructured_rooted_members() {
    assert_matches(
        "const { files: { read } } = host; read('x');",
        MatcherDecl::builder()
            .member_call_rooted("host.files.read")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn follows_a_deep_property_alias_without_changing_identity() {
    let receiver = (0..48).fold(String::from("holder"), |chain, index| {
        format!("{chain}.p{index}")
    });
    let source =
        format!("const holder = {{}}; {receiver} = app.commands; {receiver}.execute('open');");

    assert_matches(
        &source,
        MatcherDecl::builder()
            .member_call_rooted("app.commands.execute")
            .arg_static_strings(0, ["open"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn preserves_deep_module_member_provenance() {
    let member = (0..48)
        .map(|index| format!("p{index}"))
        .chain(std::iter::once(String::from("send")))
        .collect::<Vec<_>>()
        .join(".");
    let source = format!("import * as sdk from 'sdk'; sdk.{member}();");

    assert_matches(
        &source,
        MatcherDecl::builder()
            .member_call_module("sdk", &member)
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn a_deep_rooted_chain_fails_closed_after_an_earlier_prefix_mutation() {
    let suffix = (0..48)
        .map(|index| format!("p{index}"))
        .collect::<Vec<_>>()
        .join(".");
    let chain = format!("app.{suffix}.execute");
    let source = format!("app.p0.p1 = replacement; {chain}();");

    assert_matches(
        &source,
        MatcherDecl::builder()
            .member_call_rooted(&chain)
            .build()
            .unwrap(),
        0,
    );
}

#[test]
fn follows_rooted_members_called_via_sequence_expressions() {
    assert_matches(
        "(0, app.commands.execute)('open');",
        MatcherDecl::builder()
            .member_call_rooted("app.commands.execute")
            .arg_static_strings(0, ["open"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn follows_bound_rooted_members_and_their_arguments() {
    assert_matches(
        "const open = app.open.bind(app); open(vault.file);",
        MatcherDecl::builder()
            .member_call_rooted("app.open")
            .arg(0, ArgumentMatcher::rooted_expressions(["vault.file"]))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn preserves_bound_rooted_expression_arguments() {
    assert_matches(
        "const open = app.open.bind(app, vault.file); open(actual);",
        MatcherDecl::builder()
            .member_call_rooted("app.open")
            .arg(0, ArgumentMatcher::rooted_expressions(["vault.file"]))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn prepends_static_bound_arguments_before_call_arguments() {
    assert_matches(
        "const request = fetch.bind(null, '/bound'); request('/actual');",
        MatcherDecl::builder()
            .call_global("fetch")
            .arg_static_strings(0, ["/bound"])
            .build()
            .unwrap(),
        1,
    );
    assert_matches(
        "const request = fetch.bind(null, '/bound'); request('/actual');",
        MatcherDecl::builder()
            .call_global("fetch")
            .arg_static_strings(0, ["/actual"])
            .build()
            .unwrap(),
        0,
    );
    assert_matches(
        "const send = require('sdk').send.bind(null, '/bound'); send('/actual');",
        MatcherDecl::builder()
            .call_module("sdk", "send")
            .arg_static_strings(0, ["/bound"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn resolves_static_template_literals_without_substitutions() {
    assert_matches(
        "const url = `/remote`; fetch(url);",
        MatcherDecl::builder()
            .call_global("fetch")
            .arg_static_string(0)
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn resolves_constant_template_literal_substitutions() {
    assert_matches(
        "const segment = 'remote'; const url = `/${segment}`; fetch(url);",
        MatcherDecl::builder()
            .call_global("fetch")
            .arg_static_string(0)
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn resolves_static_array_property_names_through_constant_indexes() {
    assert_matches(
        "const names = ['read']; const index = 0; host.files[names[index]]('x');",
        MatcherDecl::builder()
            .member_call_rooted("host.files.read")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_immediately_invoked_arrows() {
    assert_matches(
        "((callback) => callback('/x'))(fetch);",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_immediately_invoked_functions() {
    assert_matches(
        "(function(callback) { callback('/x'); })(fetch);",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn tracks_global_callbacks_through_array_iteration() {
    assert_matches(
        "[fetch].forEach(callback => callback('/x'));",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn joins_matching_values_from_finite_array_callbacks() {
    assert_matches(
        "[fetch, fetch].forEach(callback => callback('/x'));",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
    assert_matches(
        "[fetch, local].forEach(callback => callback('/x'));",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        0,
    );
}

#[test]
fn tracks_global_callbacks_through_promise_handlers() {
    assert_matches(
        "Promise.resolve(fetch).then(callback => callback('/x'));",
        MatcherDecl::builder()
            .call_global("fetch")
            .build()
            .expect("valid matcher declaration"),
        1,
    );
}

#[test]
fn tracks_rooted_arguments_through_destructured_parameters() {
    assert_matches(
        "function open({ file }) { app.open(file); } open({ file: vault.file });",
        MatcherDecl::builder()
            .member_call_rooted("app.open")
            .arg(0, ArgumentMatcher::rooted_expressions(["vault.file"]))
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_const_spreads() {
    assert_matches(
        "const base = { url: '/x' }; const options = { ...base, method: 'GET' }; client.request(options);",
        MatcherDecl::builder()
            .member_call_rooted("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_object_assign() {
    assert_matches(
        "const options = Object.assign({}, { url: '/x', method: 'GET' }); client.request(options);",
        MatcherDecl::builder()
            .member_call_rooted("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

#[test]
fn tracks_object_argument_keys_through_member_function_aliases() {
    assert_matches(
        "const request = client.request; request({ url: '/x', method: 'GET' });",
        MatcherDecl::builder()
            .member_call_rooted("client.request")
            .arg_object_keys(0, ["url", "method"])
            .build()
            .unwrap(),
        1,
    );
}

/// Build the source/configuration/sink flow used by flow-provenance tests.
fn script_insertion_matcher() -> MatcherDecl {
    MatcherDecl::from_object_flow(
        &ObjectFlowMatcher::builder("script insertion")
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
