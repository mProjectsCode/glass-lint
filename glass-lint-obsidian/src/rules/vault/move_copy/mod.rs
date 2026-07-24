//! Obsidian vault move/copy rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted vault `rename` and `copy` calls plus
/// `app.fileManager.renameFile`. Rooted provenance follows `this.app`, direct
/// receiver aliases, static computed properties, source-ordered reassignment,
/// and lexical shadowing. Arguments, returned objects, and unlisted methods
/// are intentionally not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.move-copy")
        .description("Moves or copies vault files")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.rename")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.copy")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.fileManager.renameFile")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
