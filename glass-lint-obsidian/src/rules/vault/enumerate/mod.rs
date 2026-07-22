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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getFiles")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getMarkdownFiles")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getAllLoadedFiles")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getAllFolders")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getFolderByPath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getFileByPath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getAbstractFileByPath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.recurseChildren")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.getRoot")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
