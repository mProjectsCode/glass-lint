use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.resource-url")
        .label("Accesses attachment resource paths")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.getResourcePath"))
        .matcher(Matcher::rooted_member_call(
            "app.vault.adapter.getResourcePath",
        ))
        .matcher(Matcher::string_literal("obsidian://"))
        .build()
        .unwrap()
}
