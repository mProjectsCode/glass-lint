use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("network.request")
        .label("Uses Obsidian request APIs")
        .category("network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_member_call("obsidian", "request"))
        .matcher(Matcher::module_member_call("obsidian", "requestUrl"))
        .build()
        .unwrap()
}
