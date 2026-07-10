use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
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
