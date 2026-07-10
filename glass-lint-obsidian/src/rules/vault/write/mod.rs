use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.write")
        .label("Writes vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.create"))
        .matcher(Matcher::rooted_member_call("app.vault.createBinary"))
        .matcher(Matcher::rooted_member_call("app.vault.modify"))
        .matcher(Matcher::rooted_member_call("app.vault.modifyBinary"))
        .matcher(Matcher::rooted_member_call("app.vault.append"))
        .matcher(Matcher::rooted_member_call("app.vault.appendBinary"))
        .matcher(Matcher::rooted_member_call("app.vault.process"))
        .matcher(Matcher::rooted_member_call("app.vault.createFolder"))
        .build()
        .unwrap()
}
