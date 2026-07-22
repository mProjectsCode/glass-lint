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
        .declaration(MatcherDecl::global_call("request"))
        .declaration(MatcherDecl::global_call("requestUrl"))
        .declaration(MatcherDecl::module_member_call("obsidian", "request"))
        .declaration(MatcherDecl::module_member_call("obsidian", "requestUrl"))
        .build()
        .unwrap()
}
