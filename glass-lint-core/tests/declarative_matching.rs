//! Declarative matcher behavior exercised through the public provider API.

use std::collections::BTreeSet;

use glass_lint_core::{
    Linter, RuleCatalog,
    rules::{
        CallMatcher, Confidence, FlowMatcher, FlowValueMatcher, Matcher, MemberCallMatcher, Rule,
        Severity,
    },
};

struct Classification {
    finding_count: usize,
    rule_ids: BTreeSet<String>,
}

impl Classification {
    fn has_capability(&self, id: &str) -> bool {
        self.rule_ids.contains(&format!("test:{id}"))
    }
}

fn rule(id: &str) -> glass_lint_core::rules::Builder {
    Rule::builder(id)
        .label(id)
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

fn classify(source: &str, rules: &[Rule]) -> Classification {
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(catalog).lint(source, "matcher.js");
    Classification {
        finding_count: report.findings.len(),
        rule_ids: report
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str().to_owned())
            .collect(),
    }
}

fn assert_capability_count(result: &Classification, id: &str, expected: usize) {
    assert!(result.has_capability(id));
    assert_eq!(result.finding_count, expected);
}
#[test]
fn resolves_module_provenance_and_rejects_local_lookalikes() {
    let rules = [rule("test.module")
        .matcher(Matcher::module_call("example-sdk", "send"))
        .build()
        .unwrap()];
    let result = classify(
        "import { send as sdkSend } from 'example-sdk'; sdkSend(); function send() {} send();",
        &rules,
    );
    assert_capability_count(&result, "test.module", 1);
}

#[test]
fn resolves_commonjs_destructured_module_exports() {
    let rules = [rule("test.module")
        .matcher(Matcher::module_call("example-sdk", "send"))
        .build()
        .unwrap()];
    let result = classify(
        "const { send: sdkSend } = require('example-sdk'); sdkSend();",
        &rules,
    );
    assert_capability_count(&result, "test.module", 1);
}

#[test]
fn follows_rooted_aliases_and_reassignment_order() {
    let rules = [rule("test.alias")
        .matcher(Matcher::rooted_member_call("host.files.read"))
        .build()
        .unwrap()];
    let result = classify(
        "let files = host.files; files.read(); files = local; files.read();",
        &rules,
    );
    assert_capability_count(&result, "test.alias", 1);
}

#[test]
fn rejects_aliases_after_shadowing_reassignment() {
    let rules = [rule("test.fetch")
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap()];
    let result = classify(
        "let send = fetch; send('/remote'); send = localFetch; send('/local');",
        &rules,
    );
    assert_capability_count(&result, "test.fetch", 1);
}

#[test]
fn matches_static_string_arguments_but_rejects_dynamic_strings() {
    let rules = [rule("test.fetch-url")
        .matcher(CallMatcher::global("fetch").static_string_arg(0))
        .build()
        .unwrap()];
    let result = classify("fetch('/literal'); fetch('/' + dynamic);", &rules);
    assert_capability_count(&result, "test.fetch-url", 1);
}

#[test]
fn callable_transforms_use_effective_target_arguments() {
    let rule = [rule("test.callable")
        .matcher(CallMatcher::global("fetch").arg_string(0, ["/call", "/apply", "/optional"]))
        .build()
        .unwrap()];
    let result = classify(
        "const args = ['/apply']; fetch.call(null, '/call'); fetch.apply(null, args); fetch?.call(null, '/optional'); fetch.call(null, dynamic);",
        &rule,
    );
    assert_capability_count(&result, "test.callable", 3);
}

#[test]
fn future_declarations_fail_closed_at_the_use_position() {
    let rules = [
        rule("test.require")
            .matcher(Matcher::import("sdk"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "require('sdk').send(); const require = localRequire; fetch('/before'); const fetch = localFetch; fetch('/after');",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn numeric_addition_is_not_a_static_property_string() {
    let rules = [rule("test.member")
        .matcher(Matcher::rooted_member_call("app.12"))
        .build()
        .unwrap()];
    assert_eq!(
        classify("app[1 + 2]();", &rules).finding_count,
        0,
        "numeric addition must not be coerced into string concatenation"
    );
}

#[test]
fn tracks_rooted_expression_arguments_through_aliases() {
    let rules = [rule("test.arg-flow")
        .matcher(MemberCallMatcher::rooted_chain("app.open").arg_rooted_exprs(0, ["vault.file"]))
        .build()
        .unwrap()];
    let result = classify(
        "const file = vault.file; const opener = app; opener.open(file);",
        &rules,
    );
    assert_capability_count(&result, "test.arg-flow", 1);
}

#[test]
fn tracks_simple_parameter_aliases_into_named_functions() {
    let rules = [rule("test.fetch")
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap()];
    let result = classify(
        "function invoke(callback) { callback('/remote'); } invoke(fetch);",
        &rules,
    );
    assert_capability_count(&result, "test.fetch", 1);
}

#[test]
fn tracks_parameter_aliases_into_arrow_functions() {
    let rules = [rule("test.fetch")
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap()];
    let result = classify(
        "const invoke = (callback) => callback('/remote'); invoke(fetch);",
        &rules,
    );
    assert_capability_count(&result, "test.fetch", 1);
}

#[test]
fn matches_optional_chained_calls_with_static_arguments() {
    let rules = [rule("test.optional")
        .matcher(MemberCallMatcher::rooted_chain("app.commands.execute").arg_string(0, ["open"]))
        .build()
        .unwrap()];
    let result = classify(
        "const commands = app.commands; commands?.execute?.('open');",
        &rules,
    );
    assert_capability_count(&result, "test.optional", 1);
}

#[test]
fn resolves_literal_computed_properties_through_constant_aliases() {
    let rules = [rule("test.computed")
        .matcher(Matcher::rooted_member_call("window.fetch"))
        .build()
        .unwrap()];
    let result = classify("const method = 'fetch'; window[method]('/remote');", &rules);
    assert_capability_count(&result, "test.computed", 1);
}

#[test]
fn reuses_constant_object_arguments_for_key_matching() {
    let rules = [rule("test.object-arg")
        .matcher(
            MemberCallMatcher::rooted_chain("client.request").arg_object_keys(0, ["url", "method"]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const options = { url: '/remote', method: 'GET' }; client.request(options);",
        &rules,
    );
    assert_capability_count(&result, "test.object-arg", 1);
}

#[test]
fn rejects_reassigned_static_values() {
    let string_rules = [rule("test.fetch-url")
        .matcher(CallMatcher::global("fetch").static_string_arg(0))
        .build()
        .unwrap()];
    let object_rules = [rule("test.object-arg")
        .matcher(MemberCallMatcher::rooted_chain("client.request").arg_object_keys(0, ["url"]))
        .build()
        .unwrap()];

    assert_eq!(
        classify(
            "let url = '/remote'; url = dynamic; fetch(url);",
            &string_rules
        )
        .finding_count,
        0
    );
    assert_eq!(
        classify(
            "let options = { url: '/remote' }; options = dynamic; client.request(options);",
            &object_rules
        )
        .finding_count,
        0
    );
}

#[test]
fn rejects_static_shapes_after_a_property_write() {
    let rules = [rule("test.object-arg")
        .matcher(
            MemberCallMatcher::rooted_chain("client.request").arg_object_keys(0, ["url", "method"]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const options = { url: '/remote', method: 'GET' }; options.method = dynamic; client.request(options);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn projects_const_object_aliases_into_destructured_parameters() {
    let rules = [rule("test.arg-flow")
        .matcher(MemberCallMatcher::rooted_chain("app.open").arg_rooted_exprs(0, ["vault.file"]))
        .build()
        .unwrap()];
    let result = classify(
        "function open({ file }) { app.open(file); } const options = { file: vault.file }; open(options);",
        &rules,
    );
    assert_eq!(result.finding_count, 1);
}

#[test]
fn tracks_configured_values_into_later_member_sinks() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement('script'); script.src = getUrl(); document.head.appendChild(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_state_does_not_cross_conditional_branches_or_duplicate_sinks() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    assert_eq!(
        classify(
            "let script; if (condition) { script = document.createElement('script'); script.src = url; } else { script = local; } document.head.appendChild(script);",
            &rules
        )
        .finding_count,
        0
    );
    assert_eq!(
        classify(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
            &rules
        )
        .finding_count,
        1
    );
}

#[test]
fn value_flow_respects_reassignment_and_order() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "let script = document.createElement('script'); script.src = getUrl(); script = document.createElement('div'); document.head.appendChild(script);
         const future = document.createElement('script'); document.head.appendChild(future); future.src = getUrl();",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_supports_member_call_configuration_and_helper_sinks() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .member_call_config(
                    "setAttribute",
                    [
                        (0, FlowValueMatcher::StaticExact(vec!["src".into()])),
                        (1, FlowValueMatcher::Any),
                    ],
                )
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "function appendToHead(node) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.setAttribute('src', getUrl()); appendToHead(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_helpers_are_scope_and_assignment_aware() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         function local() { function append(node) { localSink(node); }
             const script = document.createElement('script'); script.src = url; append(script); }
         append = localAppend;
         const other = document.createElement('script'); other.src = url; append(other);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_supports_const_arrow_helper_sinks() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const appendToHead = node => document.head.appendChild(node);
         const script = document.createElement('script'); script.src = getUrl(); appendToHead(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn helper_summaries_fail_closed_for_incompatible_invocations() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("script insertion".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["script"])
                .property_write("src", FlowValueMatcher::Any)
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url;
         append(); append(script, extra);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_static_prefix_requires_static_values() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("remote element".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["img"])
                .property_write(
                    "src",
                    FlowValueMatcher::StaticPrefix(vec!["https://".into(), "http://".into()]),
                )
                .sink_member_call_arg_indices(["document.body.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const remote = document.createElement('img'); remote.src = 'https://example.com/a.png'; document.body.appendChild(remote);
         const local = document.createElement('img'); local.src = '/a.png'; document.body.appendChild(local);
         const dynamic = document.createElement('img'); dynamic.src = getUrl(); document.body.appendChild(dynamic);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_can_require_all_requirements() {
    let rules = [rule("test.flow")
        .matcher(
            FlowMatcher::new("remote stylesheet".to_string())
                .source_member_call("document.createElement")
                .source_arg_string(0, ["link"])
                .property_write(
                    "rel",
                    FlowValueMatcher::StaticExact(vec!["stylesheet".into()]),
                )
                .property_write(
                    "href",
                    FlowValueMatcher::StaticPrefix(vec!["https://".into()]),
                )
                .require_all()
                .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const good = document.createElement('link'); good.rel = 'stylesheet'; good.href = 'https://example.com/a.css'; document.head.appendChild(good);
         const missing = document.createElement('link'); missing.href = 'https://example.com/a.css'; document.head.appendChild(missing);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}
