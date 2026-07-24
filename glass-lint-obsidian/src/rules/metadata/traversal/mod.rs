//! Obsidian metadata-map traversal rule definition.

use glass_lint_core::rules::{ArgumentMatcher, Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.entries")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.keys")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.values")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.getOwnPropertyNames")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.getOwnPropertySymbols")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Object.getOwnPropertyDescriptors")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Reflect.ownKeys")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.keys")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.entries")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.values")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.getOwnPropertyNames")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.getOwnPropertySymbols")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Object.getOwnPropertyDescriptors")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("global.Reflect.ownKeys")
                .arg(0, ArgumentMatcher::rooted_expressions(METADATA_MAPS))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
