//! Obsidian metadata-map traversal rule definition.

use glass_lint_core::rules::{ArgumentMatcher, Confidence, EventQuery, Rule, Severity};

const METADATA_MAPS: [&str; 2] = [
    "app.metadataCache.resolvedLinks",
    "app.metadataCache.unresolvedLinks",
];
const METADATA_TRAVERSALS: &[&str] = &[
    "Object.entries",
    "Object.keys",
    "Object.values",
    "Object.getOwnPropertyNames",
    "Object.getOwnPropertySymbols",
    "Object.getOwnPropertyDescriptors",
    "Reflect.ownKeys",
    "global.Object.keys",
    "global.Object.entries",
    "global.Object.values",
    "global.Object.getOwnPropertyNames",
    "global.Object.getOwnPropertySymbols",
    "global.Object.getOwnPropertyDescriptors",
    "global.Reflect.ownKeys",
];

fn metadata_traversal(path: &str) -> glass_lint_core::rules::QueryDecl {
    EventQuery::member_call_rooted(path)
        .map(|query| {
            query
                .with_arg(
                    0,
                    ArgumentMatcher::rooted_expressions(METADATA_MAPS).unwrap(),
                )
                .unwrap()
                .into_query()
        })
        .unwrap()
}

/// Detects Object and Reflect key/value enumeration methods when their first
/// argument has proven rooted provenance from `resolvedLinks` or
/// `unresolvedLinks`. The enumeration call itself is syntactic; local
/// lookalikes, dynamic arguments, unlisted metadata maps, and reassigned
/// aliases are excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("metadata.traversal")
        .description("Traverses metadata cache maps")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .queries(METADATA_TRAVERSALS.iter().copied().map(metadata_traversal))
        .build()
        .unwrap()
}
