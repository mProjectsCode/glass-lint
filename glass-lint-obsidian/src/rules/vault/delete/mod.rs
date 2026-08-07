//! Obsidian vault deletion rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects rooted calls to the vault `delete`/`trash` APIs and the file
/// manager's `trashFile` API. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::catalog_builder("vault.delete")
        .description("Deletes or trashes vault files")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .query(EventQuery::member_call_rooted("app.vault.delete"))
        .query(EventQuery::member_call_rooted("app.vault.trash"))
        .query(EventQuery::member_call_rooted("app.fileManager.trashFile"))
        .build()
        .unwrap()
}
