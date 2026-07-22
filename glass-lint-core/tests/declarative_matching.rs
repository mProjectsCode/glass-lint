//! Declarative matcher behavior exercised through the public provider API.
//!
//! The helpers intentionally build a new catalog per case so rule selection,
//! environment configuration, and finding counts remain independently visible.

use std::collections::BTreeSet;

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    rules::{
        CallMatcher, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher, MemberCallMatcher,
        ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule, ValueMatcher,
    },
};

#[path = "declarative_matching/flow.rs"]
mod flow;

#[path = "support/mod.rs"]
mod support;

use support::rule;

struct Classification {
    finding_count: usize,
    rule_ids: BTreeSet<String>,
}

impl Classification {
    fn has_capability(&self, id: &str) -> bool {
        self.rule_ids.contains(&format!("test:{id}"))
    }
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
            .build(),
    )
}

/// Lint one source with exactly the supplied rules and record matched IDs.
fn classify(source: &str, rules: &[Rule]) -> Classification {
    classify_with_environment(source, rules, support::test_environment())
}

fn classify_with_environment(
    source: &str,
    rules: &[Rule],
    environment: glass_lint_core::Environment,
) -> Classification {
    let catalog = glass_lint_core::RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
        vec![catalog],
        environment,
    ))
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

#[test]
fn canonicalizes_configured_global_object_aliases_for_rooted_members() {
    let rules = [
        rule("test.navigator-call")
            .matcher(Matcher::rooted_member_call("navigator.sendBeacon"))
            .build()
            .unwrap(),
        rule("test.navigator-read")
            .matcher(Matcher::rooted_member_read("navigator.userAgent"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "navigator.sendBeacon('/bare'); globalThis.navigator.sendBeacon('/global');\n\
         window.navigator.sendBeacon('/window'); self.navigator.sendBeacon('/self');\n\
         globalThis.navigator.userAgent;",
        &rules,
    );
    assert_eq!(result.finding_count, 5);
}

#[test]
fn rooted_configured_global_member_calls_match_direct_globals() {
    let mut environment = support::test_environment();
    environment.add_global("crypto").unwrap();
    let catalog = RuleCatalog::new(
        "test",
        vec![
            rule("crypto")
                .matcher(Matcher::rooted_member_call("crypto.subtle.digest"))
                .build()
                .unwrap(),
        ],
    )
    .unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet("crypto.subtle.digest('SHA-256', bytes);", "matcher.js")
        .unwrap();
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn rooted_global_member_survives_unrelated_crypto_imports() {
    let mut environment = support::test_environment();
    environment.add_global("crypto").unwrap();
    let rules = [rule("crypto")
        .matcher(Matcher::rooted_member_call("crypto.subtle.digest"))
        .build()
        .unwrap()];
    let result = classify_with_environment(
        "import c from 'node:crypto'; crypto.subtle.digest('SHA-256', bytes);",
        &rules,
        environment,
    );
    assert_eq!(result.finding_count, 1);
}

#[test]
fn rooted_member_read_matches_direct_read() {
    let rules = [rule("document")
        .matcher(Matcher::rooted_member_read("document.onkeydown"))
        .build()
        .unwrap()];
    assert_eq!(classify("document.onkeydown;", &rules).finding_count, 1);
}

#[test]
fn rooted_global_object_aliases_respect_restricted_members_and_mutations() {
    let mut environment = Environment::default();
    environment.add_global("navigator").unwrap();
    environment
        .add_global_object_with_members("foreignWindow", ["fetch"])
        .unwrap();
    let rules = [
        rule("test.navigator")
            .matcher(Matcher::rooted_member_call("navigator.sendBeacon"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .matcher(Matcher::rooted_member_call("fetch"))
            .build()
            .unwrap(),
    ];
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet(
            "foreignWindow.navigator.sendBeacon('/no');\n\
             globalThis.navigator.sendBeacon('/yes');\n\
             navigator.sendBeacon = local; navigator.sendBeacon('/no');\n\
             globalThis.navigator.sendBeacon('/no');\n\
             foreignWindow.fetch('/yes');",
            "matcher.js",
        )
        .unwrap();
    assert_eq!(report.files[0].findings.len(), 2);
    assert_eq!(
        report.files[0]
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<Vec<_>>(),
        vec!["test:test.navigator", "test:test.fetch"]
    );
}

#[test]
fn rooted_global_object_alias_mutations_invalidate_the_canonical_root() {
    let mut environment = Environment::default();
    environment.add_global("navigator").unwrap();
    let rules = [rule("test.navigator")
        .matcher(Matcher::rooted_member_call("navigator.sendBeacon"))
        .build()
        .unwrap()];
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet(
            "globalThis.navigator = replacement;\n\
             navigator.sendBeacon('/bare');\n\
             window.navigator = replacement;\n\
             globalThis.navigator.sendBeacon('/alias');",
            "matcher.js",
        )
        .unwrap();
    assert!(report.files[0].findings.is_empty());
}

#[test]
fn extracted_instance_callables_follow_aliases_and_bind_but_not_reassignment() {
    let rules = [rule("instance")
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "addCommand",
        ))
        .build()
        .unwrap()];
    let result = classify(
        "import { Plugin } from 'obsidian';\n\
         class TestPlugin extends Plugin {\n\
           run() {\n\
             const add = this.addCommand; add({});\n\
             add.call(this, {}); add.apply(this, [{}]);\n\
             const bound = this.addCommand.bind(this); bound({});\n\
             this.addCommand = replacement; this.addCommand({});\n\
           }\n\
         }",
        &rules,
    );
    assert_capability_count(&result, "instance", 4);
}

#[test]
fn package_import_patterns_match_subpaths_without_lookalikes() {
    let rules = [rule("package")
        .matcher(Matcher::package_import("@scope/pkg"))
        .matcher(Matcher::package_import("openai"))
        .build()
        .unwrap()];
    let result = classify(
        "import root from '@scope/pkg';\n\
         import subpath from '@scope/pkg/client';\n\
         import lookalike from '@scope/pkg-extra';\n\
         import root from 'openai';\n\
         import subpath from 'openai/helpers';\n\
         import lookalike from 'openai-extra';",
        &rules,
    );
    assert_capability_count(&result, "package", 4);
}

#[test]
fn package_provenance_matches_exports_and_namespace_members_at_boundaries() {
    let rules = [rule("package-provenance")
        .matcher(Matcher::package_call("sdk", "send"))
        .matcher(Matcher::package_member_call("sdk", "client.request"))
        .matcher(Matcher::package_member_read("sdk", "version"))
        .build()
        .unwrap()];
    let result = classify(
        "import { send } from 'sdk/client';\n\
         import * as client from 'sdk/client';\n\
         send(); client.client.request(); client.version;\n\
         import { send as fake } from 'sdk-extra'; fake();",
        &rules,
    );
    assert_capability_count(&result, "package-provenance", 3);
}

#[test]
fn associates_static_option_properties_with_their_call_sink() {
    let rules = [rule("string-use")
        .matcher(CallMatcher::global("fetch").arg_object_property_value(
            1,
            "url",
            ValueMatcher::static_string().contains_any(["localhost"]),
        ))
        .build()
        .unwrap()];
    let result = classify(
        "fetch('/remote', { url: 'http://localhost:3000' });\n\
         fetch('/remote', { url: getUrl() });",
        &rules,
    );
    assert_capability_count(&result, "string-use", 1);
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
        .matcher(Matcher::rooted_member_call("fetch"))
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
