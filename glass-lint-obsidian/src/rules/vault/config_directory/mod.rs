use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.config-directory")
        .label("References .obsidian configuration paths")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_literal(".obsidian/"))
        .matcher(Matcher::string_literal(".obsidian\\\\"))
        .build()
        .unwrap()
}
