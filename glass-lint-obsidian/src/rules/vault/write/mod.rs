//! Obsidian vault write rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to the eight configured vault write APIs: `create`,
/// `createBinary`, `modify`, `modifyBinary`, `append`, `appendBinary`,
/// `process`, and `createFolder`. Provenance follows `this.app`, receiver
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Local lookalikes, dynamic or unlisted members, and
/// call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.write")
        .description("Writes vault files")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.create")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.createBinary")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.modify")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.modifyBinary")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.append")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.appendBinary")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.process")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.vault.createFolder")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
