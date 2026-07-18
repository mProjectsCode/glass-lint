//! Obsidian metadata-map traversal rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

const METADATA_MAPS: [&str; 2] = [
    "app.metadataCache.resolvedLinks",
    "app.metadataCache.unresolvedLinks",
];

/// Detects Object and Reflect key/value enumeration methods when their first
/// argument has proven rooted provenance from `resolvedLinks` or
/// `unresolvedLinks`. The enumeration call itself is syntactic; local
/// lookalikes, dynamic arguments, unlisted metadata maps, and reassigned
/// aliases are excluded.
pub fn rule() -> Rule {
    Rule::builder("metadata.traversal")
        .description("Traverses metadata cache maps")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.entries").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.keys").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.values").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.getOwnPropertyNames").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.getOwnPropertySymbols").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Object.getOwnPropertyDescriptors").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("Reflect.ownKeys").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(rooted_global_traversal("Object.keys"))
        .matcher(rooted_global_traversal("Object.entries"))
        .matcher(rooted_global_traversal("Object.values"))
        .matcher(rooted_global_traversal("Object.getOwnPropertyNames"))
        .matcher(rooted_global_traversal("Object.getOwnPropertySymbols"))
        .matcher(rooted_global_traversal("Object.getOwnPropertyDescriptors"))
        .matcher(rooted_global_traversal("Reflect.ownKeys"))
        .matcher(rooted_global_this_traversal("Object.keys"))
        .matcher(rooted_global_this_traversal("Object.entries"))
        .matcher(rooted_global_this_traversal("Object.values"))
        .matcher(rooted_global_this_traversal("Object.getOwnPropertyNames"))
        .matcher(rooted_global_this_traversal("Object.getOwnPropertySymbols"))
        .matcher(rooted_global_this_traversal(
            "Object.getOwnPropertyDescriptors",
        ))
        .matcher(rooted_global_this_traversal("Reflect.ownKeys"))
        .build()
        .unwrap()
}

fn rooted_global_traversal(method: &str) -> Matcher {
    Matcher::from(
        MemberCallMatcher::rooted(format!("global.{method}")).arg_rooted_exprs(0, METADATA_MAPS),
    )
}

fn rooted_global_this_traversal(method: &str) -> Matcher {
    Matcher::from(
        MemberCallMatcher::rooted(format!("globalThis.{method}"))
            .arg_rooted_exprs(0, METADATA_MAPS),
    )
}
