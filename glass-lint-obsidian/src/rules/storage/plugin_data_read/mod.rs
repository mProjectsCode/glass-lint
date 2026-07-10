use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("storage.plugin-data-read")
        .label("Reads plugin data")
        .category("storage")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_call("this.loadData"))
        .build()
        .unwrap()
}
