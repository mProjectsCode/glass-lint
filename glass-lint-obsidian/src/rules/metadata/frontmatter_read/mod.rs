use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted reads of `app.metadataCache.getFileCache.frontmatter`,
/// including aliases and static computed properties. It does not infer
/// frontmatter from arbitrary objects, does not follow shadowed or reassigned
/// aliases, and does not analyze the cached value itself.
pub(crate) fn rule() -> Rule {
    Rule::builder("metadata.frontmatter-read")
        .label("Reads cached frontmatter")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.frontmatter",
        ))
        .build()
        .unwrap()
}
