//! Obsidian vault enumeration rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to the configured vault lookup and enumeration methods:
/// `getFiles`, `getMarkdownFiles`, `getAllLoadedFiles`, `getAllFolders`,
/// `getFolderByPath`, `getFileByPath`, `getAbstractFileByPath`,
/// `recurseChildren`, and `getRoot`. The matcher follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing, but does not analyze arguments or other vault APIs.
pub fn rule() -> Rule {
    Rule::builder("vault.enumerate")
        .description("Enumerates vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("app.vault.getFiles"))
        .declaration(MatcherDecl::rooted_member_call(
            "app.vault.getMarkdownFiles",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.vault.getAllLoadedFiles",
        ))
        .declaration(MatcherDecl::rooted_member_call("app.vault.getAllFolders"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.getFolderByPath"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.getFileByPath"))
        .declaration(MatcherDecl::rooted_member_call(
            "app.vault.getAbstractFileByPath",
        ))
        .declaration(MatcherDecl::rooted_member_call("app.vault.recurseChildren"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.getRoot"))
        .build()
        .unwrap()
}
