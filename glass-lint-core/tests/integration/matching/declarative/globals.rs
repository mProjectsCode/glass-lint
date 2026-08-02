use super::*;
#[test]
fn canonicalizes_configured_global_object_aliases_for_rooted_members() {
    let rules = [
        rule("test.navigator-call")
            .query(EventQuery::member_call_rooted("navigator.sendBeacon"))
            .build()
            .unwrap(),
        rule("test.navigator-read")
            .query(EventQuery::member_read_rooted("navigator.userAgent"))
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
                .query(EventQuery::member_call_rooted("crypto.subtle.digest"))
                .build()
                .unwrap(),
        ],
    )
    .unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_source(source(
            "matcher.js",
            "crypto.subtle.digest('SHA-256', bytes);",
        ))
        .unwrap();
    assert_eq!(report.files()[0].findings().len(), 1);
}

#[test]
fn rooted_global_member_survives_unrelated_crypto_imports() {
    let mut environment = support::test_environment();
    environment.add_global("crypto").unwrap();
    let rules = [rule("crypto")
        .query(EventQuery::member_call_rooted("crypto.subtle.digest"))
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
        .query(EventQuery::member_read_rooted("document.onkeydown"))
        .build()
        .unwrap()];
    assert_eq!(classify("document.onkeydown;", &rules).finding_count, 1);
}

#[test]
fn rooted_property_write_matches_direct_assignment() {
    let rules = [rule("document")
        .query(EventQuery::property_write_rooted("document.onkeydown"))
        .build()
        .unwrap()];
    assert_eq!(
        classify("document.onkeydown = handler;", &rules).finding_count,
        1
    );
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
            .query(EventQuery::member_call_rooted("navigator.sendBeacon"))
            .build()
            .unwrap(),
        rule("test.fetch")
            .query(EventQuery::member_call_rooted("fetch"))
            .build()
            .unwrap(),
    ];
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_source(source(
            "matcher.js",
            "foreignWindow.navigator.sendBeacon('/no');\n\
             globalThis.navigator.sendBeacon('/yes');\n\
             navigator.sendBeacon = local; navigator.sendBeacon('/no');\n\
             globalThis.navigator.sendBeacon('/no');\n\
             foreignWindow.fetch('/yes');",
        ))
        .unwrap();
    assert_eq!(report.files()[0].findings().len(), 2);
    assert_eq!(
        report.files()[0]
            .findings()
            .iter()
            .map(|finding| finding.rule_id().as_str())
            .collect::<Vec<_>>(),
        vec!["test:test.navigator", "test:test.fetch"]
    );
}

#[test]
fn rooted_global_object_alias_mutations_invalidate_the_canonical_root() {
    let mut environment = Environment::default();
    environment.add_global("navigator").unwrap();
    let rules = [rule("test.navigator")
        .query(EventQuery::member_call_rooted("navigator.sendBeacon"))
        .build()
        .unwrap()];
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_source(source(
            "matcher.js",
            "globalThis.navigator = replacement;\n\
             navigator.sendBeacon('/bare');\n\
             window.navigator = replacement;\n\
             globalThis.navigator.sendBeacon('/alias');",
        ))
        .unwrap();
    assert!(report.files()[0].findings().is_empty());
}

#[test]
fn extracted_instance_callables_follow_aliases_and_bind_but_not_reassignment() {
    let rules = [rule("instance")
        .query(QueryDecl::member_call_instance(
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
        .query(EventQuery::import_package("@scope/pkg"))
        .query(EventQuery::import_package("openai"))
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
        .query(EventQuery::call_package("sdk", "send"))
        .query(EventQuery::member_call_package("sdk", "client.request"))
        .query(EventQuery::member_read_package("sdk", "version"))
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
        .query(
            EventQuery::call_global("fetch")
                .unwrap()
                .with_arg_object_property_value(
                    1,
                    "url",
                    ValueMatcher::static_string()
                        .contains_any(["localhost"])
                        .unwrap(),
                )
                .unwrap()
                .into_query(),
        )
        .build()
        .unwrap()];
    let result = classify(
        "fetch('/remote', { url: 'http://localhost:3000' });\n\
         fetch('/remote', { url: getUrl() });",
        &rules,
    );
    assert_capability_count(&result, "string-use", 1);
}
