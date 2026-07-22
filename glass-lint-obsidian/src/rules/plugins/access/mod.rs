//! Obsidian plugin-manager access rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects reads from Obsidian's plugin manager: instances, manifests, and
/// enabled-plugin state. The rooted collection read intentionally also covers
/// static and dynamic plugin keys without guessing the key's value.
pub fn rule() -> Rule {
    Rule::builder("plugins.access")
        .description("Accesses other plugins")
        .category("plugins")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("app.plugins.getPlugin"))
        .declaration(MatcherDecl::rooted_member_read("app.plugins.plugins"))
        .declaration(MatcherDecl::rooted_member_read("app.plugins.manifests"))
        .declaration(MatcherDecl::rooted_member_read(
            "app.plugins.enabledPlugins",
        ))
        .build()
        .unwrap()
}
