use super::*;
#[test]
fn catalogs_are_namespaced() {
    assert!(
        js_catalog()
            .rule_ids()
            .all(|rule| rule.as_str().starts_with("js:"))
    );
    assert!(
        browser_catalog()
            .rule_ids()
            .all(|rule| rule.as_str().starts_with("browser:"))
    );
    assert!(
        electron_catalog()
            .rule_ids()
            .all(|rule| rule.as_str().starts_with("electron:"))
    );
    assert!(
        node_catalog()
            .rule_ids()
            .all(|rule| rule.as_str().starts_with("node:"))
    );
    let environment = electron_environment();
    assert!(environment.global_bindings().any(|name| name == "fetch"));
    assert!(environment.global_objects().any(|name| name == "window"));
}

#[test]
fn caller_can_extend_the_electron_environment() {
    let mut environment = electron_environment();
    environment.add_global_object("activeWindow").unwrap();
    let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
        vec![js_catalog(), browser_catalog()],
        environment,
    ))
    .unwrap();
    let report = linter
        .lint_source(SourceFile::new("main.js", "activeWindow.fetch('/x')").unwrap())
        .unwrap();
    assert!(
        report.files()[0]
            .findings()
            .iter()
            .any(|finding| finding.rule_id().as_str() == "browser:network.request")
    );
}

#[test]
fn node_web_crypto_global_is_rooted() {
    let linter = glass_lint_core::Linter::new(node_config()).unwrap();
    let report = linter
        .lint_source(SourceFile::new("main.js", "crypto.subtle.digest('SHA-256', bytes)").unwrap())
        .unwrap();
    assert!(
        report.files()[0]
            .findings()
            .iter()
            .any(|finding| finding.rule_id().as_str() == "node:crypto.operation")
    );
}

#[test]
fn node_web_crypto_global_survives_catalog_imports() {
    let linter = glass_lint_core::Linter::new(node_config()).unwrap();
    let report = linter
            .lint_source(SourceFile::new(
                "main.js",
                "import c from 'node:crypto'; import * as cryptoPromises from 'crypto/promises'; import * as nodeCryptoPromises from 'node:crypto/promises'; import coreCrypto from 'crypto'; import cryptoJs from 'crypto-js'; crypto.subtle.digest('SHA-256', bytes);",
            ).unwrap())
            .unwrap();
    assert!(
        report.files()[0]
            .findings()
            .iter()
            .any(|finding| { finding.rule_id().as_str() == "node:crypto.operation" })
    );
}

#[test]
fn node_crypto_fixture_uses_rooted_web_crypto() {
    let linter = glass_lint_core::Linter::new(node_config()).unwrap();
    let source = include_str!("rules/node/crypto_operation/positive.js");
    let report = linter
        .lint_source(SourceFile::new("positive.js", source).unwrap())
        .unwrap();
    let count = report.files()[0]
        .findings()
        .iter()
        .filter(|finding| finding.rule_id().as_str() == "node:crypto.operation")
        .count();
    assert!(count >= 29, "expected rooted calls in fixture, got {count}");
}
