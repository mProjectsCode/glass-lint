//! Obsidian vault write rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::member_call_rooted("app.vault.create"))
        .query(QueryDecl::member_call_rooted("app.vault.createBinary"))
        .query(QueryDecl::member_call_rooted("app.vault.modify"))
        .query(QueryDecl::member_call_rooted("app.vault.modifyBinary"))
        .query(QueryDecl::member_call_rooted("app.vault.append"))
        .query(QueryDecl::member_call_rooted("app.vault.appendBinary"))
        .query(QueryDecl::member_call_rooted("app.vault.process"))
        .query(QueryDecl::member_call_rooted("app.vault.createFolder"))
        .build()
        .unwrap()
}
