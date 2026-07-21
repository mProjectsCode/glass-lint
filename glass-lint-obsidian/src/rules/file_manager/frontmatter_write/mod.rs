//! Obsidian frontmatter-write rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to the rooted Obsidian API
/// `app.fileManager.processFrontMatter`, including proven aliases and static
/// computed properties. Shadowed `app` bindings, reassigned aliases, dynamic
/// or unlisted properties, and the callback's contents are outside the rule.
pub fn rule() -> Rule {
    Rule::builder("file-manager.frontmatter-write")
        .description("Updates frontmatter through Obsidian APIs")
        .category("file-manager")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "app.fileManager.processFrontMatter",
        ))
        .build()
        .unwrap()
}
