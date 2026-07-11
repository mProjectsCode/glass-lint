use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.addCommand()` registration call, including a
/// statically computed `addCommand` property. This medium-confidence heuristic
/// does not prove an Obsidian plugin receiver and does not follow aliases,
/// shadowing, or reassignment; other receivers, dynamic properties, and
/// near-name methods are excluded, and command descriptors are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.command")
        .label("Registers commands")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.addCommand"))
        .build()
        .unwrap()
}
