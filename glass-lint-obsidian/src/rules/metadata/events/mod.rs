use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects rooted `app.metadataCache.on` registrations only when the first
/// argument is a literal event name: `changed`, `deleted`, or `resolved`.
/// Rooted aliases are followed; shadowing, reassignment, dynamic event values,
/// computed member chains, and other event names are excluded.
pub fn rule() -> Rule {
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
