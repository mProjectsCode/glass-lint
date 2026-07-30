//! Obsidian network-request rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects calls to the exact `request` and `requestUrl` exports of the
/// `obsidian` module or the corresponding globals injected into the plugin's
/// current realm. ESM/CommonJS and callable aliases retain provenance, while
/// similar modules, shadowing, reassignment, and foreign-realm lookalikes are
/// excluded; request arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("network.request")
        .description("Uses Obsidian request APIs")
        .category(Category::new("network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("request"))
        .query(EventQuery::call_global("requestUrl"))
        .query(EventQuery::member_call_module("obsidian", "request"))
        .query(EventQuery::member_call_module("obsidian", "requestUrl"))
        .build()
        .unwrap()
}
