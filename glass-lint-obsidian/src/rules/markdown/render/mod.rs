//! Markdown renderer rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects module-proven calls to `MarkdownRenderer.render`. Same-shaped local
/// receivers and unproven bare aliases are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.render")
        .description("Renders markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::member_call_module(
            "obsidian",
            "MarkdownRenderer.render",
        ))
        .build()
        .unwrap()
}
