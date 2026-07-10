use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("metadata.events")
        .label("Registers metadata cache events")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::member_call(
            MemberCallMatcher::rooted_chain("app.metadataCache.on")
                .arg_string(0, ["changed", "deleted", "resolved"]),
        ))
        .build()
        .unwrap()
}
