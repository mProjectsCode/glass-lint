//! Obsidian vault deletion rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to the vault `delete`/`trash` APIs and the file
/// manager's `trashFile` API. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.delete")
        .description("Deletes or trashes vault files")
        .category(Category::new("vault").unwrap())
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.delete")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.trash")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.fileManager.trashFile")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
