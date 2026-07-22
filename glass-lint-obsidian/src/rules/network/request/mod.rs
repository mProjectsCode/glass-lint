//! Obsidian network-request rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the exact `request` and `requestUrl` exports of the
/// `obsidian` module or the corresponding globals injected into the plugin's
/// current realm. ESM/CommonJS and callable aliases retain provenance, while
/// similar modules, shadowing, reassignment, and foreign-realm lookalikes are
/// excluded; request arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("network.request")
        .description("Uses Obsidian request APIs")
        .category("network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_global("request")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .call_global("requestUrl")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "request")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "requestUrl")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
