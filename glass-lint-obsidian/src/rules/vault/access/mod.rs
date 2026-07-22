//! Obsidian vault-root access rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects reads of the rooted `app.vault` object, including `this.app` and
/// direct aliases of the root receiver plus static computed properties. The
/// matcher tracks source-ordered root reassignment and lexical shadowing, but
/// does not follow a bare alias after reading the value; vault methods and
/// argument/value semantics are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.access")
        .description("Accesses Obsidian vault APIs")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.vault")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
