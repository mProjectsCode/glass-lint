use glass_lint_core::rules::{CallMatcher, Confidence, Matcher, Rule, Severity};

/// Detects calls to the exact `request` and `requestUrl` exports of the
/// `obsidian` module or the corresponding globals injected into the plugin's
/// current realm. ESM/CommonJS and callable aliases retain provenance, while
/// similar modules, shadowing, reassignment, and foreign-realm lookalikes are
/// excluded; request arguments are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("network.request")
        .label("Uses Obsidian request APIs")
        .category("network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(CallMatcher::global("request"))
        .matcher(CallMatcher::global("requestUrl"))
        .matcher(Matcher::module_member_call("obsidian", "request"))
        .matcher(Matcher::module_member_call("obsidian", "requestUrl"))
        .build()
        .unwrap()
}
