//! Obsidian frontmatter-write rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects calls to the rooted Obsidian API
/// `app.fileManager.processFrontMatter`, including proven aliases and static
/// computed properties. Shadowed `app` bindings, reassigned aliases, dynamic
/// or unlisted properties, and the callback's contents are outside the rule.
pub fn rule() -> Rule {
    Rule::catalog_builder("file-manager.frontmatter-write")
        .description("Updates frontmatter through Obsidian APIs")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "app.fileManager.processFrontMatter",
        ))
        .build()
        .unwrap()
}
