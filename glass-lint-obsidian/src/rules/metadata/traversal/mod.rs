use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("metadata.traversal")
        .label("Traverses metadata cache maps")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("Object.entries").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("Object.keys").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("Object.values").arg_rooted_exprs(
                0,
                [
                    "app.metadataCache.resolvedLinks",
                    "app.metadataCache.unresolvedLinks",
                ],
            ),
        ))
        .build()
        .unwrap()
}
