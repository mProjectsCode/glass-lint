use super::*;
#[test]
fn resolves_module_provenance_and_rejects_local_lookalikes() {
    let rules = [rule("test.module")
        .query(EventQuery::call_module("example-sdk", "send"))
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
        .query(EventQuery::call_module("example-sdk", "send"))
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
        .query(EventQuery::member_call_rooted("host.files.read"))
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
        .query(EventQuery::call_global("fetch"))
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
        .query(
            EventQuery::call_global("fetch")
                .unwrap()
                .with_arg_static_string(0)
                .unwrap()
                .into_query(),
        )
        .build()
        .unwrap()];
    let result = classify("fetch('/literal'); fetch('/' + dynamic);", &rules);
    assert_capability_count(&result, "test.fetch-url", 1);
}

#[test]
fn callable_transforms_use_effective_target_arguments() {
    let rule = [rule("test.callable")
        .query(
            EventQuery::call_global("fetch")
                .unwrap()
                .with_arg_static_strings(0, ["/call", "/apply", "/optional"])
                .unwrap()
                .into_query(),
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
        .query(
            EventQuery::call_global("eval")
                .unwrap()
                .with_arg_static_strings(0, ["direct", "alias", "call", "apply"])
                .unwrap()
                .into_query(),
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
        .query(EventQuery::call_global("eval"))
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
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let default_catalog = RuleCatalog::new("test", vec![rule.clone()]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(
            vec![default_catalog],
            Environment::default(),
        ))
        .unwrap()
        .lint_source(source(
            "matcher.js",
            "fetch('/unconfigured'); const run = fetch; run('/alias')",
        ))
        .unwrap()
        .files()[0]
            .findings()
            .is_empty()
    );

    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    environment.add_global_object("activeWindow").unwrap();
    let configured = RuleCatalog::new("test", vec![rule]).unwrap();
    let report = Linter::new(LinterConfig::new(vec![configured], environment))
        .unwrap()
        .lint_source(source(
            "matcher.js",
            "fetch('/direct'); activeWindow.fetch('/window')",
        ))
        .unwrap();
    assert_eq!(report.files()[0].findings().len(), 2);
}

#[test]
fn rooted_host_globals_also_require_environment_configuration() {
    let rule = rule("test.host")
        .query(EventQuery::member_call_rooted("host.open"))
        .build()
        .unwrap();
    let default_catalog = RuleCatalog::new("test", vec![rule.clone()]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(
            vec![default_catalog],
            Environment::default(),
        ))
        .unwrap()
        .lint_source(source("matcher.js", "host.open()"))
        .unwrap()
        .files()[0]
            .findings()
            .is_empty()
    );

    let mut environment = Environment::default();
    environment.add_global("host").unwrap();
    let configured = RuleCatalog::new("test", vec![rule]).unwrap();
    assert_eq!(
        Linter::new(LinterConfig::new(vec![configured], environment))
            .unwrap()
            .lint_source(source("matcher.js", "host.open()"))
            .unwrap()
            .files()[0]
            .findings()
            .len(),
        1
    );
}

#[test]
fn custom_global_objects_do_not_make_unconfigured_members_global() {
    let rule = rule("test.fetch")
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global_object("activeWindow").unwrap();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    assert!(
        Linter::new(LinterConfig::new(vec![catalog], environment))
            .unwrap()
            .lint_source(source("matcher.js", "activeWindow.fetch('/unknown')"))
            .unwrap()
            .files()[0]
            .findings()
            .is_empty()
    );
}

#[test]
fn future_declarations_fail_closed_at_the_use_position() {
    let rules = [
        rule("test.require")
            .query(EventQuery::import_exact("sdk"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .query(EventQuery::call_global("fetch"))
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
            .query(EventQuery::import_exact("sdk"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .query(EventQuery::call_global("fetch"))
            .build()
            .unwrap(),
        rule("test.global-fetch")
            .query(EventQuery::member_call_rooted("globalThis.fetch"))
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
        .query(EventQuery::member_call_rooted("app.12"))
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
        .query(
            EventQuery::member_call_rooted("app.open")
                .unwrap()
                .with_arg(
                    0,
                    ArgumentMatcher::rooted_expressions(["vault.file"]).unwrap(),
                )
                .unwrap()
                .into_query(),
        )
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
        .query(EventQuery::call_global("fetch"))
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
        .query(EventQuery::call_global("fetch"))
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
        .query(EventQuery::call_global("fetch"))
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
        .query(
            EventQuery::member_call_rooted("app.commands.execute")
                .unwrap()
                .with_arg_static_strings(0, ["open"])
                .unwrap()
                .into_query(),
        )
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
        .query(EventQuery::member_call_rooted("fetch"))
        .build()
        .unwrap()];
    let result = classify("const method = 'fetch'; window[method]('/remote');", &rules);
    assert_capability_count(&result, "test.computed", 1);
}

#[test]
fn reuses_constant_object_arguments_for_key_matching() {
    let rules = [rule("test.object-arg")
        .query(
            EventQuery::member_call_rooted("client.request")
                .unwrap()
                .with_arg_object_keys(0, ["url", "method"])
                .unwrap()
                .into_query(),
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
        .query(
            EventQuery::call_global("fetch")
                .unwrap()
                .with_arg_static_string(0)
                .unwrap()
                .into_query(),
        )
        .build()
        .unwrap()];
    let object_rules = [rule("test.object-arg")
        .query(
            EventQuery::member_call_rooted("client.request")
                .unwrap()
                .with_arg_object_keys(0, ["url"])
                .unwrap()
                .into_query(),
        )
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
        .query(
            EventQuery::member_call_rooted("client.request")
                .unwrap()
                .with_arg_object_keys(0, ["url", "method"])
                .unwrap()
                .into_query(),
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
        .query(
            EventQuery::member_call_rooted("app.open")
                .unwrap()
                .with_arg(
                    0,
                    ArgumentMatcher::rooted_expressions(["vault.file"]).unwrap(),
                )
                .unwrap()
                .into_query(),
        )
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
        .query(QueryDecl::lifecycle(Ok(script_insertion_flow())))
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement('script'); script.src = getUrl(); document.head.appendChild(script);",
        &rules,
    );
    assert_capability_count(&result, "test.flow", 1);
}

/// Regression test: chained constructor calls (e.g. `new Menu().addItem(item)`)
/// should be tracked as instance receiver provenance but currently are not.
/// The instance is created via `new`, returned anonymously, and the member call
/// happens on the ephemeral result without an intermediate binding.
#[test]
fn instance_matchers_do_not_track_chained_constructor_calls() {
    let rules = [rule("test.instance")
        .query(QueryDecl::member_call_instance(
            "obsidian", "Menu", "addItem",
        ))
        .build()
        .unwrap()];
    let result = classify(
        "import { Menu } from 'obsidian';\n\
         new Menu().addItem(item);",
        &rules,
    );
    assert_eq!(result.finding_count, 1);
}

/// Rooted property writes should retain the same receiver identity through a
/// local alias as rooted member calls and reads do.
#[test]
fn rooted_property_writes_follow_receiver_aliases() {
    let rules = [rule("test.property-write")
        .query(EventQuery::property_write_rooted("navigator.onLine"))
        .build()
        .unwrap()];
    let result = classify("const nav = navigator; nav.onLine = value;", &rules);
    assert_capability_count(&result, "test.property-write", 1);
}

#[test]
fn rooted_property_writes_keep_receiver_identity_after_prior_writes() {
    let rules = [rule("test.repeated-property-write")
        .query(EventQuery::property_write_rooted("navigator.onLine"))
        .build()
        .unwrap()];
    let result = classify(
        "const nav = navigator;
         nav.onLine = first;
         nav.onLine = second;",
        &rules,
    );
    assert_capability_count(&result, "test.repeated-property-write", 2);
}

/// A heuristic constructor is intentionally independent of the configured
/// global environment and should match its spelling like heuristic calls do.
#[test]
fn heuristic_constructors_match_unconfigured_names() {
    let rules = [rule("test.constructor")
        .query(EventQuery::constructor_heuristic("PluginSettingTab"))
        .build()
        .unwrap()];
    let result = classify("new PluginSettingTab();", &rules);
    assert_capability_count(&result, "test.constructor", 1);
}

/// Default imports are module exports too; all three author-visible event
/// forms should preserve the `default` export identity.
#[test]
fn default_imports_preserve_module_export_identity() {
    let rules = [
        rule("test.call")
            .query(EventQuery::call_module("sdk", "default"))
            .build()
            .unwrap(),
        rule("test.construct")
            .query(EventQuery::constructor_module("sdk", "default"))
            .build()
            .unwrap(),
        rule("test.class")
            .query(EventQuery::class_module("sdk", "default"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "import DefaultExport from 'sdk';
         DefaultExport();
         new DefaultExport();
         class Child extends DefaultExport {}",
        &rules,
    );
    assert_eq!(result.finding_count, 3);
}
