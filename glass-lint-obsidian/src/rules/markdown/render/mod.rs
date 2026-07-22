//! Markdown renderer rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects module-proven calls to `MarkdownRenderer.render`. Same-shaped local
/// receivers and unproven bare aliases are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.render")
        .description("Renders markdown")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "MarkdownRenderer.render",
        ))
        .build()
        .unwrap()
}
