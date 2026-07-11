use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects string literals containing a configured Dataview or DataCore marker:
/// `dataview`, `dataviewapi`, `data-core`, or `datacore`. This medium-confidence
/// heuristic does not establish module provenance, plugin API use, aliases,
/// shadowing, or reassignment; dynamic expressions and concatenations are not
/// reconstructed, while marker substrings and static template fragments match.
pub(crate) fn rule() -> Rule {
    Rule::builder("plugins.dataview")
        .label("References Dataview or DataCore")
        .category("plugins")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_literal("dataview"))
        .matcher(Matcher::string_literal("dataviewapi"))
        .matcher(Matcher::string_literal("data-core"))
        .matcher(Matcher::string_literal("datacore"))
        .build()
        .unwrap()
}
