//! Obsidian metadata-map traversal rule definition.

use glass_lint_core::rules::{ArgumentMatcher, Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::member_call_rooted("Object.entries")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Object.keys")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Object.values")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Object.getOwnPropertyNames")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Object.getOwnPropertySymbols")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Object.getOwnPropertyDescriptors")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("Reflect.ownKeys")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.keys")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.entries")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.values")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.getOwnPropertyNames")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.getOwnPropertySymbols")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Object.getOwnPropertyDescriptors")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .query(QueryDecl::member_call_rooted("global.Reflect.ownKeys")
                .with_arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS)))
        .build()
        .unwrap()
}
