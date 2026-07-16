//! Obsidian vault deletion rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to the vault `delete`/`trash` APIs and the file
/// manager's `trashFile` API. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
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
