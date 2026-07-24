//! Obsidian plugin-manager access rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects reads from Obsidian's plugin manager: instances, manifests, and
/// enabled-plugin state. The rooted collection read intentionally also covers
/// static and dynamic plugin keys without guessing the key's value.
pub fn rule() -> Rule {
    Rule::builder("plugins.access")
        .description("Accesses other plugins")
        .category(Category::new("plugins").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.getPlugin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.plugins.plugins")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.plugins.manifests")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.plugins.enabledPlugins")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
