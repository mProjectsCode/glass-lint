//! Declarative matcher behavior exercised through the public provider API.
//!
//! The helpers intentionally build a new catalog per case so rule selection,
//! environment configuration, and finding counts remain independently visible.

use std::collections::BTreeSet;

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    rules::{
        CallMatcher, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher,
        MemberCallMatcher, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        Severity, ValueMatcher,
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

/// Build a consistent high-confidence rule for one declarative capability.
fn rule(id: &str) -> glass_lint_core::rules::Builder {
    Rule::builder(id)
        .description(id)
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

/// Construct the multi-step flow used by source/configuration/sink tests.
fn script_insertion_flow() -> Matcher {
    Matcher::from(
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
            .expect("script insertion flow should build"),
    )
}

/// Lint one source with exactly the supplied rules and record matched IDs.
fn classify(source: &str, rules: &[Rule]) -> Classification {
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], test_environment()))
        .unwrap()
        .lint_snippet(source, "matcher.js")
        .unwrap();
    Classification {
        finding_count: report.files[0].findings.len(),
        rule_ids: report.files[0]
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str().to_owned())
            .collect(),
    }
}

/// Configure only globals that the declarative cases are allowed to trust.
fn test_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "app", "client", "document", "fetch", "host", "require", "vault",
        ])
        .unwrap();
    for object in ["window", "self", "global"] {
        environment.add_global_object(object).unwrap();
    }
    environment
}

/// Require both the named capability and the exact total finding count.
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
        .matcher(CallMatcher::global("fetch").arg_static_string(0))
        .build()
        .unwrap()];
    let result = classify("fetch('/literal'); fetch('/' + dynamic);", &rules);
    assert_capability_count(&result, "test.fetch-url", 1);
}

#[test]
fn callable_transforms_use_effective_target_arguments() {
    let rule = [rule("test.callable")
        .matcher(
            CallMatcher::global("fetch").arg_static_strings(0, ["/call", "/apply", "/optional"]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "const args = ['/apply']; fetch.call(null, '/call'); fetch.apply(null, args); fetch?.call(null, '/optional'); fetch.call(null, dynamic);",
        &rule,
    );
    assert_capability_count(&result, "test.callable", 3);
}

#[test]
fn global_call_matchers_cover_proven_global_object_callable_forms() {
    let rules = [rule("test.global-callable")
        .matcher(
            CallMatcher::global("eval").arg_static_strings(0, ["direct", "alias", "call", "apply"]),
        )
        .build()
        .unwrap()];
    let result = classify(
        "globalThis.eval('direct');
         const run = window.eval; run('alias');
         self.eval.call(null, 'call');
         const args = ['apply']; global.eval.apply(null, args);",
        &rules,
    );
    assert_capability_count(&result, "test.global-callable", 4);
}

#[test]
fn global_object_callable_forms_respect_shadowing_and_property_mutation() {
    let rules = [rule("test.global-callable")
        .matcher(CallMatcher::global("eval"))
        .build()
        .unwrap()];
    let result = classify(
        "function local(window) { window.eval('local'); }
         const globals = globalThis; globals.eval = safeEval;
         globalThis.eval('mutated through alias');
         const member = 'eval'; self[member] = safeEval;
         self.eval('dynamically mutated');
         globalThis.eval = safeEval;
         globalThis.eval('mutated');",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn host_globals_require_explicit_environment_configuration() {
    let rule = rule("test.fetch")
        .matcher(CallMatcher::global("fetch"))
        .build()
        .unwrap();
    let default_catalog = RuleCatalog::new("test", vec![rule.clone()]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(
            vec![default_catalog],
            Environment::default()
        ))
        .unwrap()
        .lint_snippet(
            "fetch('/unconfigured'); const run = fetch; run('/alias')",
            "matcher.js",
        )
        .unwrap()
        .files[0]
            .findings
            .is_empty()
    );

    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    environment.add_global_object("activeWindow").unwrap();
    let configured = RuleCatalog::new("test", vec![rule]).unwrap();
    let report = Linter::new(LinterConfig::new(vec![configured], environment))
        .unwrap()
        .lint_snippet(
            "fetch('/direct'); activeWindow.fetch('/window')",
            "matcher.js",
        )
        .unwrap();
    assert_eq!(report.files[0].findings.len(), 2);
}

#[test]
fn rooted_host_globals_also_require_environment_configuration() {
    let rule = rule("test.host")
        .matcher(Matcher::rooted_member_call("host.open"))
        .build()
        .unwrap();
    let default_catalog = RuleCatalog::new("test", vec![rule.clone()]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(
            vec![default_catalog],
            Environment::default()
        ))
        .unwrap()
        .lint_snippet("host.open()", "matcher.js")
        .unwrap()
        .files[0]
            .findings
            .is_empty()
    );

    let mut environment = Environment::default();
    environment.add_global("host").unwrap();
    let configured = RuleCatalog::new("test", vec![rule]).unwrap();
    assert_eq!(
        Linter::new(LinterConfig::new(vec![configured], environment))
            .unwrap()
            .lint_snippet("host.open()", "matcher.js")
            .unwrap()
            .files[0]
            .findings
            .len(),
        1
    );
}

#[test]
fn custom_global_objects_do_not_make_unconfigured_members_global() {
    let rule = rule("test.fetch")
        .matcher(CallMatcher::global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global_object("activeWindow").unwrap();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(vec![catalog], environment))
            .unwrap()
            .lint_snippet("activeWindow.fetch('/unknown')", "matcher.js")
            .unwrap()
            .files[0]
            .findings
            .is_empty()
    );
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
fn future_declarations_shadow_all_builtin_provenance_seeds() {
    let rules = [
        rule("test.import")
            .matcher(Matcher::import("sdk"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap(),
        rule("test.global-fetch")
            .matcher(Matcher::rooted_member_call("globalThis.fetch"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "require('sdk').send(); const require = localRequire;
         __toESM(require('sdk')).send(); const __toESM = localInterop;
         Promise.resolve(fetch).then(callback => callback('/x')); const Promise = localPromise;
         globalThis.fetch('/x'); const globalThis = localGlobalThis;",
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
        .matcher(MemberCallMatcher::rooted("app.open").arg_rooted_exprs(0, ["vault.file"]))
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
fn named_helper_summaries_are_lexically_scoped() {
    let rules = [rule("test.fetch")
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap()];
    let result = classify(
        "function localScope() { function invoke(callback) { callback('/local'); } invoke(local); }
         function globalScope() { function invoke(callback) { callback('/global'); } invoke(fetch); }
         localScope(); globalScope();",
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
        .matcher(MemberCallMatcher::rooted("app.commands.execute").arg_static_strings(0, ["open"]))
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
        .matcher(MemberCallMatcher::rooted("client.request").arg_object_keys(0, ["url", "method"]))
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
        .matcher(CallMatcher::global("fetch").arg_static_string(0))
        .build()
        .unwrap()];
    let object_rules = [rule("test.object-arg")
        .matcher(MemberCallMatcher::rooted("client.request").arg_object_keys(0, ["url"]))
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
        .matcher(MemberCallMatcher::rooted("client.request").arg_object_keys(0, ["url", "method"]))
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
        .matcher(MemberCallMatcher::rooted("app.open").arg_rooted_exprs(0, ["vault.file"]))
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
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement('script'); script.src = getUrl(); document.head.appendChild(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn flow_calls_use_effective_call_and_apply_arguments() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const first = document.createElement.call(document, 'script'); first.src = url;
         document.head.appendChild.call(document.head, first);
         const args = [second]; const second = document.createElement.apply(document, ['script']);
         second.src = url; document.head.appendChild.apply(document.head, [second]);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 2);
}

#[test]
fn flow_control_boundaries_fail_closed_after_loops_try_and_destructuring() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "let loopValue; while (condition) { loopValue = document.createElement('script'); loopValue.src = url; }
         document.head.appendChild(loopValue);
         let tryValue; try { tryValue = document.createElement('script'); tryValue.src = url; } catch (error) {}
         document.head.appendChild(tryValue);
         const source = document.createElement('script'); const { node } = source; node.src = url; document.head.appendChild(node);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn flow_state_does_not_cross_conditional_branches_or_duplicate_sinks() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
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
        2
    );
}

#[test]
fn value_flow_respects_reassignment_and_order() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
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
fn flow_kills_object_state_for_compound_writes_updates_and_delete() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const compound = document.createElement('script'); compound.src = url; compound.src += suffix; document.head.appendChild(compound);
         const updated = document.createElement('script'); updated.src = url; updated.src++; document.head.appendChild(updated);
         const deleted = document.createElement('script'); deleted.src = url; delete deleted.src; document.head.appendChild(deleted);",
        &rules,
    );
    assert_eq!(result.finding_count, 0);
}

#[test]
fn value_flow_supports_member_call_configuration_and_helper_sinks() {
    let rules = [rule("test.flow")
        .matcher(Matcher::from(
            ObjectFlowMatcher::builder("script insertion")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                ))
                .configured_by(FlowCondition::event(
                    ObjectEventMatcher::member_call("setAttribute")
                        .arg(0, ValueMatcher::static_string().equals("src"))
                        .arg(1, ValueMatcher::any_value()),
                ))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.head.appendChild"),
                    0,
                )]))
                .build()
                .unwrap(),
        ))
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
        .matcher(script_insertion_flow())
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
        .matcher(script_insertion_flow())
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
fn value_flow_projects_nested_destructured_helper_arguments() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append([{ node }]) { document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url;
         append([{ node: script }]);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
    assert_eq!(
        classify(
            "function append([{ node }]) { document.head.appendChild(node); }
             const script = document.createElement('script'); script.src = url;
             append([{ other: script }]);",
            &rules,
        )
        .finding_count,
        0
    );
}

#[test]
fn value_flow_reaches_sinks_through_mutually_recursive_helpers() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function first(node) { second(node); }
         function second(node) { first(node); document.head.appendChild(node); }
         const script = document.createElement('script'); script.src = url; first(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn value_flow_uses_precise_helper_parameter_defaults() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement('script'); script.src = url;
         function append(node = script) { document.head.appendChild(node); }
         append();",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);

    let nested_default = classify(
        "const script = document.createElement('script'); script.src = url;
         function append({ node = script }) { document.head.appendChild(node); }
         append({});",
        &rules,
    );
    assert_capability_count(&nested_default, "test.flow", 1);

    let rest_parameter = classify(
        "const script = document.createElement('script'); script.src = url;
         function append(...nodes) { document.head.appendChild(nodes[0]); }
         append(script);",
        &rules,
    );
    assert_capability_count(&rest_parameter, "test.flow", 1);
}

#[test]
fn value_flow_follows_function_aliases_by_function_id() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
        .build()
        .unwrap()];
    let result = classify(
        "function append(node) { document.head.appendChild(node); }
         const alias = append;
         const script = document.createElement('script'); script.src = url; alias(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

#[test]
fn helper_summaries_fail_closed_for_incompatible_invocations() {
    let rules = [rule("test.flow")
        .matcher(script_insertion_flow())
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
        .matcher(Matcher::from(
            ObjectFlowMatcher::builder("remote element")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("img")),
                ))
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "src",
                    ValueMatcher::static_string().starts_with_any(["https://", "http://"]),
                )))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.body.appendChild"),
                    0,
                )]))
                .build()
                .unwrap(),
        ))
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
        .matcher(Matcher::from(
            ObjectFlowMatcher::builder("remote stylesheet")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("link")),
                ))
                .configured_by(FlowCondition::all_of([
                    ObjectEventMatcher::property_write(
                        "rel",
                        ValueMatcher::static_string().equals("stylesheet"),
                    ),
                    ObjectEventMatcher::property_write(
                        "href",
                        ValueMatcher::static_string().starts_with_any(["https://"]),
                    ),
                ]))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.head.appendChild"),
                    0,
                )]))
                .build()
                .unwrap(),
        ))
        .build()
        .unwrap()];
    let result = classify(
        "const good = document.createElement('link'); good.rel = 'stylesheet'; good.href = 'https://example.com/a.css'; document.head.appendChild(good);
         const missing = document.createElement('link'); missing.href = 'https://example.com/a.css'; document.head.appendChild(missing);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}
