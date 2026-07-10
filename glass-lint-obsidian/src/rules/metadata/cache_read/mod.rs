use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("metadata.cache-read")
        .label("Reads Obsidian metadata cache")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.metadataCache"))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.resolvedLinks",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.unresolvedLinks",
        ))
        .matcher(Matcher::rooted_member_call(
            "app.metadataCache.getFileCache",
        ))
        .matcher(Matcher::rooted_member_call("app.metadataCache.getCache"))
        .matcher(Matcher::rooted_member_call(
            "app.metadataCache.getFirstLinkpathDest",
        ))
        .build()
        .unwrap()
}
