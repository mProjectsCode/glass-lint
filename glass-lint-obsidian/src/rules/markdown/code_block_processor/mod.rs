use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("markdown.code-block-processor")
        .label("Registers markdown code-block processors")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call(
            "this.registerMarkdownCodeBlockProcessor",
        ))
        .build()
        .unwrap()
}
