use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.addRibbonIcon()` registration call, including
/// statically computed property names. This medium-confidence heuristic does
/// not prove an Obsidian receiver and does not follow aliases or reassignment;
/// other receivers, dynamic properties, and near-name methods are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.ribbon")
        .label("Registers ribbon icons")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.addRibbonIcon"))
        .build()
        .unwrap()
}
