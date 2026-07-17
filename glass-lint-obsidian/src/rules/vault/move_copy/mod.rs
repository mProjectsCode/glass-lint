//! Obsidian vault move/copy rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted vault `rename` and `copy` calls plus
/// `app.fileManager.renameFile`. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.move-copy")
        .description("Moves or copies vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.rename"))
        .matcher(Matcher::rooted_member_call("app.vault.copy"))
        .matcher(Matcher::rooted_member_call("app.fileManager.renameFile"))
        .build()
        .unwrap()
}
