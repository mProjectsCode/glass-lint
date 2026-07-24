//! Obsidian metadata-cache event rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted `app.metadataCache.on` registrations only when the first
/// argument is a literal event name: `changed`, `deleted`, `resolve`, or
/// `resolved`.
/// Rooted aliases are followed; shadowing, reassignment, dynamic event values,
/// computed member chains, and other event names are excluded.
pub fn rule() -> Rule {
    Rule::builder("metadata.events")
        .description("Registers metadata cache events")
        .category(Category::new("metadata").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.metadataCache.on")
                .arg_static_strings(0, ["changed", "deleted", "resolve", "resolved"])
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
