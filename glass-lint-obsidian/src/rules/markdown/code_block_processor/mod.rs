//! Markdown code-block processor rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic chain `this.registerMarkdownCodeBlockProcessor`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver; aliases
/// and reassignment are not followed. Static computed names are accepted while
/// dynamic properties and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.code-block-processor")
        .label("Registers markdown code-block processors")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerMarkdownCodeBlockProcessor",
        ))
        .build()
        .unwrap()
}
