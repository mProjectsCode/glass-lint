use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.delete")
        .label("Deletes or trashes vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .matcher(Matcher::rooted_member_call("app.vault.delete"))
        .matcher(Matcher::rooted_member_call("app.vault.trash"))
        .matcher(Matcher::rooted_member_call("app.fileManager.trashFile"))
        .build()
        .unwrap()
}
