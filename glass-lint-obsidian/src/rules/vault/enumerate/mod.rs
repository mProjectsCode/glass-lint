use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to the six configured vault enumeration methods:
/// `getFiles`, `getMarkdownFiles`, `getAllLoadedFiles`, `getAllFolders`,
/// `getFolderByPath`, and `getRoot`. The matcher follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing, but does not analyze arguments or other vault APIs.
pub fn rule() -> Rule {
    Rule::builder("vault.enumerate")
        .label("Enumerates vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.getFiles"))
        .matcher(Matcher::rooted_member_call("app.vault.getMarkdownFiles"))
        .matcher(Matcher::rooted_member_call("app.vault.getAllLoadedFiles"))
        .matcher(Matcher::rooted_member_call("app.vault.getAllFolders"))
        .matcher(Matcher::rooted_member_call("app.vault.getFolderByPath"))
        .matcher(Matcher::rooted_member_call("app.vault.getRoot"))
        .build()
        .unwrap()
}
