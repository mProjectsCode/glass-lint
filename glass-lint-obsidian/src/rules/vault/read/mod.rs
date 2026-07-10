use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.read")
        .label("Reads vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.read"))
        .matcher(Matcher::rooted_member_call("app.vault.cachedRead"))
        .matcher(Matcher::rooted_member_call("app.vault.readBinary"))
        .build()
        .unwrap()
}
