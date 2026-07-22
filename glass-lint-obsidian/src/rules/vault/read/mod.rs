//! Obsidian vault read rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to vault `read`, `cachedRead`, and `readBinary`.
/// Provenance follows `this.app`, direct receiver aliases, static computed
/// properties, bounded rooted argument flow, source-ordered reassignment, and
/// lexical shadowing. Arguments and other read-like methods are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.read")
        .description("Reads vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.read")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.cachedRead")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.readBinary")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
