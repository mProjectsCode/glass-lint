use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("markdown.link")
        .label("Uses markdown link helpers")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::module_member_call("obsidian", "parseLinktext"))
        .matcher(Matcher::module_member_call("obsidian", "normalizePath"))
        .matcher(Matcher::module_member_call("obsidian", "getLinkpath"))
        .build()
        .unwrap()
}
