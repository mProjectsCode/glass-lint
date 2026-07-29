//! Obsidian metadata-cache event rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects rooted `app.metadataCache.on` registrations only when the first
/// argument is a literal event name: `changed`, `deleted`, `finished`,
/// `resolve`, or `resolved`.
/// Rooted aliases are followed; shadowing, reassignment, dynamic event values,
/// computed member chains, and other event names are excluded.
pub fn rule() -> Rule {
    Rule::builder("metadata.events")
        .description("Registers metadata cache events")
        .category(Category::new("metadata").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(
            EventQuery::member_call_rooted("app.metadataCache.on")
                .map(|q| {
                    q.with_arg_static_strings(
                        0,
                        ["changed", "deleted", "finished", "resolve", "resolved"],
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
