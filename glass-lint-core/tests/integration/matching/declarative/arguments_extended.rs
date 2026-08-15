use super::*;

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
