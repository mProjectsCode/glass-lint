use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("dynamic-code.script-injection")
        .label("Injects executable script elements")
        .category("browser/dom")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("document.createElement")
                .arg_string(0, ["script"]),
        ))
        .build()
        .unwrap()
}
