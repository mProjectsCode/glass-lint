//! Obsidian plugin load/unload rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects plugin-manager and returned-plugin load/unload operations.
pub fn rule() -> Rule {
    Rule::builder("plugins.load-unload")
        .description("Loads or unloads plugins at runtime")
        .category("plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.loadPlugin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.plugins.unloadPlugin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("app.plugins.getPlugin", "load")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("app.plugins.getPlugin", "unload")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("app.plugins.plugins", "load")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("app.plugins.plugins", "unload")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
