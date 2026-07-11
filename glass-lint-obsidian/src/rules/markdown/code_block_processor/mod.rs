use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic chain `this.registerMarkdownCodeBlockProcessor`.
/// It is a medium-confidence heuristic rather than proof of an Obsidian
/// plugin receiver, so aliases and reassignment are not followed. Static
/// computed names are accepted; other receivers, dynamic properties, and
/// near-name methods are excluded.
pub(crate) fn rule() -> Rule {
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
