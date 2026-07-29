//! Obsidian metadata-map traversal rule definition.

use glass_lint_core::rules::{ArgumentMatcher, Category, Confidence, EventQuery, Rule, Severity};

const METADATA_MAPS: [&str; 2] = [
    "app.metadataCache.resolvedLinks",
    "app.metadataCache.unresolvedLinks",
];

/// Detects Object and Reflect key/value enumeration methods when their first
/// argument has proven rooted provenance from `resolvedLinks` or
/// `unresolvedLinks`. The enumeration call itself is syntactic; local
/// lookalikes, dynamic arguments, unlisted metadata maps, and reassigned
/// aliases are excluded.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("metadata.traversal")
        .description("Traverses metadata cache maps")
        .category(Category::new("metadata").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(
            EventQuery::member_call_rooted("Object.entries")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Object.keys")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Object.values")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Object.getOwnPropertyNames")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Object.getOwnPropertySymbols")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Object.getOwnPropertyDescriptors")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("Reflect.ownKeys")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.keys")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.entries")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.values")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.getOwnPropertyNames")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.getOwnPropertySymbols")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Object.getOwnPropertyDescriptors")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("global.Reflect.ownKeys")
                .map(|q| {
                    q.with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
