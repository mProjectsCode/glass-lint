//! Obsidian vault enumeration rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted calls to the configured vault lookup and enumeration methods:
/// `getFiles`, `getMarkdownFiles`, `getAllLoadedFiles`, `getAllFolders`,
/// `getFolderByPath`, `getFileByPath`, `getAbstractFileByPath`,
/// `recurseChildren`, and `getRoot`. The matcher follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing, but does not analyze arguments or other vault APIs.
pub fn rule() -> Rule {
    Rule::builder("vault.enumerate")
        .description("Enumerates vault files")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("app.vault.getFiles"))
        .query(QueryDecl::member_call_rooted("app.vault.getMarkdownFiles"))
        .query(QueryDecl::member_call_rooted("app.vault.getAllLoadedFiles"))
        .query(QueryDecl::member_call_rooted("app.vault.getAllFolders"))
        .query(QueryDecl::member_call_rooted("app.vault.getFolderByPath"))
        .query(QueryDecl::member_call_rooted("app.vault.getFileByPath"))
        .query(QueryDecl::member_call_rooted(
            "app.vault.getAbstractFileByPath",
        ))
        .query(QueryDecl::member_call_rooted("app.vault.recurseChildren"))
        .query(QueryDecl::member_call_rooted("app.vault.getRoot"))
        .build()
        .unwrap()
}
