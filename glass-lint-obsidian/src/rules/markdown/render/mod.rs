//! Markdown renderer rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects syntactic calls to `MarkdownRenderer.render` and
/// `obsidian.MarkdownRenderer.render`, plus module-proven namespace calls. The
/// heuristic forms do not establish renderer provenance, do not follow aliases,
/// and report same-shaped local receivers; other methods and dynamic
/// properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.render")
        .description("Renders markdown")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("MarkdownRenderer.render"))
        .matcher(Matcher::heuristic_member_call(
            "obsidian.MarkdownRenderer.render",
        ))
        .matcher(Matcher::module_member_call(
            "obsidian",
            "MarkdownRenderer.render",
        ))
        .build()
        .unwrap()
}
