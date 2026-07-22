//! Obsidian vault-adapter access rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects reads of rooted `app.vault.adapter`, including `this.app`, direct
/// receiver aliases, and static computed properties. Source-ordered root
/// reassignment and lexical shadowing are respected, while a bare adapter
/// alias is not followed after initialization; later method names and
/// arguments are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.adapter")
        .description("Uses adapter-level vault filesystem APIs")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.vault.adapter")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
