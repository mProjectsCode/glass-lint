//! Obsidian vault deletion rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to the vault `delete`/`trash` APIs and the file
/// manager's `trashFile` API. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.delete")
        .description("Deletes or trashes vault files")
        .category("vault")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .declaration(MatcherDecl::rooted_member_call("app.vault.delete"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.trash"))
        .declaration(MatcherDecl::rooted_member_call("app.fileManager.trashFile"))
        .build()
        .unwrap()
}
