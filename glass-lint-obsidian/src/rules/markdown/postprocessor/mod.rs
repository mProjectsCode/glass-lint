use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic member chain `this.registerMarkdownPostProcessor`.
/// This medium-confidence heuristic does not prove an Obsidian plugin
/// receiver and does not follow aliases or reassignment. Static computed names
/// are accepted; other receivers, dynamic properties, and near-name methods
/// are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("markdown.postprocessor")
        .label("Registers markdown postprocessors")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call(
            "this.registerMarkdownPostProcessor",
        ))
        .build()
        .unwrap()
}
