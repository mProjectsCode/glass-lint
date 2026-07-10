use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
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
