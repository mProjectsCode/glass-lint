use super::*;
#[test]
fn lifecycle_sources_follow_optional_calls() {
    let rules = [rule("test.optional-source")
        .query(QueryDecl::lifecycle(Ok(script_insertion_flow())))
        .build()
        .unwrap()];
    let result = classify(
        "const script = document.createElement?.('script');
         script.src = url;
         document.head.appendChild(script);",
        &rules,
    );
    assert_capability_count(&result, "test.optional-source", 1);
}

#[test]
fn import_queries_match_dynamic_imports() {
    let rules = [
        rule("test.exact")
            .query(EventQuery::import_exact("sdk"))
            .build()
            .unwrap(),
        rule("test.package")
            .query(EventQuery::import_package("sdk"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "await import('sdk');
         await import('sdk/client');",
        &rules,
    );
    // The exact rule reports `sdk` once; the package rule reports both
    // `sdk` and `sdk/client`.
    assert_eq!(result.finding_count, 3);
}

#[test]
fn returned_member_queries_follow_optional_producer_calls() {
    let rules = [rule("test.returned-optional")
        .query(QueryDecl::member_call_returned(
            "document.createElement",
            "appendChild",
        ))
        .build()
        .unwrap()];
    let result = classify(
        "const node = document.createElement?.('div');
         node.appendChild(child);",
        &rules,
    );
    assert_capability_count(&result, "test.returned-optional", 1);
}

#[test]
fn heuristic_class_queries_match_instanceof_operands() {
    let rules = [rule("test.class")
        .query(EventQuery::class_heuristic("PluginSettingTab"))
        .build()
        .unwrap()];
    let result = classify("value instanceof PluginSettingTab;", &rules);
    assert_capability_count(&result, "test.class", 1);
}

#[test]
fn string_queries_match_constant_compositions() {
    let rules = [rule("test.string")
        .query(EventQuery::string_contains("token"))
        .build()
        .unwrap()];
    let result = classify(
        "const concatenated = 'to' + 'ken';
         const templated = `to${'ken'}`;",
        &rules,
    );
    assert_capability_count(&result, "test.string", 2);
}

#[test]
fn string_query_findings_cover_only_the_matched_text() {
    let rule = rule("test.string-location")
        .query(EventQuery::string_contains("localhost"))
        .build()
        .unwrap();
    let report = support::lint_report(r#"const endpoint = "http://localhost:3000";"#, rule);
    let range = report.files()[0].findings()[0].location().range();
    assert_eq!(range.start().column(), 26);
    assert_eq!(range.end().column(), 35);
}

#[test]
fn former_private_network_sentinel_is_plain_literal_text() {
    let rule = rule("test.sentinel")
        .query(EventQuery::string_contains(
            "__glass_lint_private_network_literal__",
        ))
        .build()
        .unwrap();
    let report = support::lint_report(
        r#"const endpoint = "http://192.168.1.2:8080";"#,
        rule.clone(),
    );
    assert_eq!(report.files()[0].findings().len(), 0);
    let report = support::lint_report(
        r#"const value = "__glass_lint_private_network_literal__";"#,
        rule,
    );
    assert_eq!(report.files()[0].findings().len(), 1);
}

#[test]
fn string_query_aliases_keep_the_defining_literal_location() {
    let rule = rule("test.string-alias-location")
        .query(EventQuery::string_contains("localhost"))
        .build()
        .unwrap();
    let report = support::lint_report(
        r#"const endpoint = "http://localhost:3000"; const alias = endpoint; use(alias);"#,
        rule,
    );
    assert_eq!(report.files()[0].findings().len(), 1);
    let range = report.files()[0].findings()[0].location().range();
    assert_eq!(range.start().column(), 26);
    assert_eq!(range.end().column(), 35);
}

#[test]
fn private_network_findings_cover_only_the_address() {
    let rule = rule("test.private-network")
        .query(EventQuery::string_private_network_address())
        .build()
        .unwrap();
    let report = support::lint_report(r#"const endpoint = "http://192.168.1.2:8080/path";"#, rule);
    let range = report.files()[0].findings()[0].location().range();
    assert_eq!(range.start().column(), 26);
    assert_eq!(range.end().column(), 37);
    assert!(
        report.files()[0].findings()[0].evidence().traces()[0].steps()[0]
            .message()
            .contains("private network address")
    );
}

#[test]
fn heuristic_constructors_follow_transparent_callee_wrappers() {
    let rules = [rule("test.constructor-wrappers")
        .query(EventQuery::constructor_heuristic("PluginSettingTab"))
        .build()
        .unwrap()];
    let result = classify(
        "new (PluginSettingTab)();
         new (0, PluginSettingTab)();",
        &rules,
    );
    assert_capability_count(&result, "test.constructor-wrappers", 2);
}

#[test]
fn default_import_aliases_preserve_module_export_identity() {
    let rules = [
        rule("test.call-alias")
            .query(EventQuery::call_module("sdk", "default"))
            .build()
            .unwrap(),
        rule("test.construct-alias")
            .query(EventQuery::constructor_module("sdk", "default"))
            .build()
            .unwrap(),
        rule("test.class-alias")
            .query(EventQuery::class_module("sdk", "default"))
            .build()
            .unwrap(),
    ];
    let result = classify(
        "import DefaultExport from 'sdk';
         const callAlias = DefaultExport;
         const ConstructorAlias = DefaultExport;
         const BaseAlias = DefaultExport;
         callAlias();
         new ConstructorAlias();
         class Child extends BaseAlias {}",
        &rules,
    );
    assert_eq!(result.finding_count, 3);
}

#[test]
fn returned_member_queries_follow_optional_receiver_calls() {
    let rules = [rule("test.returned-optional-receiver")
        .query(QueryDecl::member_call_returned(
            "document.createElement",
            "appendChild",
        ))
        .build()
        .unwrap()];
    let result = classify(
        "const node = document?.createElement('div');
         node.appendChild(child);",
        &rules,
    );
    assert_capability_count(&result, "test.returned-optional-receiver", 1);
}

#[test]
fn heuristic_class_queries_match_superclass_operands() {
    let rules = [rule("test.class-super")
        .query(EventQuery::class_heuristic("PluginSettingTab"))
        .build()
        .unwrap()];
    let result = classify("class Child extends PluginSettingTab {}", &rules);
    assert_capability_count(&result, "test.class-super", 1);
}

#[test]
fn string_queries_match_compositions_of_constant_aliases() {
    let rules = [rule("test.string-aliases")
        .query(EventQuery::string_contains("token"))
        .build()
        .unwrap()];
    let result = classify(
        "const prefix = 'to';
         const suffix = 'ken';
         const concatenated = prefix + suffix;
         const templated = `${prefix}${suffix}`;",
        &rules,
    );
    assert_capability_count(&result, "test.string-aliases", 2);
}

#[test]
fn compound_assignments_are_rooted_property_writes() {
    let rules = [rule("test.compound-property-write")
        .query(EventQuery::property_write_rooted("document.cookie"))
        .build()
        .unwrap()];
    let result = classify(
        "document.cookie += suffix;
         document.cookie ||= fallback;",
        &rules,
    );
    assert_capability_count(&result, "test.compound-property-write", 2);
}

#[test]
fn named_default_imports_share_default_import_member_semantics() {
    let rules = [rule("test.default-member-parity")
        .query(EventQuery::member_call_module("sdk", "send"))
        .build()
        .unwrap()];
    let result = classify(
        "import DefaultSyntax from 'sdk';
         import { default as NamedSyntax } from 'sdk';
         DefaultSyntax.send();
         NamedSyntax.send();",
        &rules,
    );
    assert_capability_count(&result, "test.default-member-parity", 2);
}

#[test]
fn default_import_bound_callables_preserve_default_export_identity() {
    let rules = [rule("test.default-bind")
        .query(EventQuery::call_module("sdk", "default"))
        .build()
        .unwrap()];
    let result = classify(
        "import DefaultExport from 'sdk';
         const bound = DefaultExport.bind(null);
         bound();",
        &rules,
    );
    assert_capability_count(&result, "test.default-bind", 1);
}

#[test]
fn extracted_deep_default_import_members_preserve_module_provenance() {
    let rules = [rule("test.deep-default-member")
        .query(EventQuery::call_module("sdk", "client.send"))
        .build()
        .unwrap()];
    let result = classify(
        "import sdk from 'sdk';
         const send = sdk.client.send;
         send();",
        &rules,
    );
    assert_capability_count(&result, "test.deep-default-member", 1);
}

#[test]
fn import_queries_match_static_template_dynamic_imports() {
    let rules = [rule("test.template-import")
        .query(EventQuery::import_exact("sdk"))
        .build()
        .unwrap()];
    let result = classify(
        "await import(`sdk`);
         await import('s' + 'dk');",
        &rules,
    );
    assert_capability_count(&result, "test.template-import", 2);
}

#[test]
fn esm_export_bound_callables_preserve_module_provenance() {
    let rules = [rule("test.esm-bind")
        .query(EventQuery::call_module("sdk", "send"))
        .build()
        .unwrap()];
    let result = classify(
        "import { send } from 'sdk';
         const bound = send.bind(null);
         bound();",
        &rules,
    );
    assert_capability_count(&result, "test.esm-bind", 1);
}

#[test]
fn extracted_named_export_members_preserve_deep_module_provenance() {
    let rules = [rule("test.deep-named-member")
        .query(EventQuery::call_module("sdk", "client.send"))
        .build()
        .unwrap()];
    let result = classify(
        "import { client } from 'sdk';
         const send = client.send;
         send();",
        &rules,
    );
    assert_capability_count(&result, "test.deep-named-member", 1);
}
