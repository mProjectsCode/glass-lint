use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("file-manager.frontmatter-write")
        .label("Updates frontmatter through Obsidian APIs")
        .category("file-manager/frontmatter-write")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "app.fileManager.processFrontMatter",
        ))
        .build()
        .unwrap()
}
