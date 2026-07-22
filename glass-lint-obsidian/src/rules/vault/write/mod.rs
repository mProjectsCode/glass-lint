//! Obsidian vault write rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to the eight configured vault write APIs: `create`,
/// `createBinary`, `modify`, `modifyBinary`, `append`, `appendBinary`,
/// `process`, and `createFolder`. Provenance follows `this.app`, receiver
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Local lookalikes, dynamic or unlisted members, and
/// call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.write")
        .description("Writes vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("app.vault.create"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.createBinary"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.modify"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.modifyBinary"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.append"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.appendBinary"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.process"))
        .declaration(MatcherDecl::rooted_member_call("app.vault.createFolder"))
        .build()
        .unwrap()
}
