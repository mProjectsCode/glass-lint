use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("markdown.render")
        .label("Renders markdown")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("MarkdownRenderer.render"))
        .matcher(Matcher::heuristic_member_call(
            "obsidian.MarkdownRenderer.render",
        ))
        .build()
        .unwrap()
}
