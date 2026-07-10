use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.move-copy")
        .label("Moves or copies vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.rename"))
        .matcher(Matcher::rooted_member_call("app.vault.copy"))
        .matcher(Matcher::rooted_member_call("app.fileManager.renameFile"))
        .build()
        .unwrap()
}
