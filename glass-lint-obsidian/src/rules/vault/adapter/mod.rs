use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.adapter")
        .label("Uses adapter-level vault filesystem APIs")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.vault.adapter"))
        .build()
        .unwrap()
}
