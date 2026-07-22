//! Obsidian workspace-open rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to `app.workspace.openLinkText` and
/// `app.workspace.getLeaf.openFile`. Provenance follows `this.app`, workspace
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Dynamic or unlisted members, local lookalikes, and
/// call arguments are not analyzed. Returned leaves are followed through
/// simple aliases while reassignment and shadowing remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("workspace.open")
        .description("Opens files through the workspace")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.openLinkText",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.getLeaf.openFile",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.workspace.getLeaf",
            "openFile",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.workspace.getLeafById",
            "openFile",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.workspace.getLeftLeaf",
            "openFile",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.workspace.getRightLeaf",
            "openFile",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.workspace.ensureSideLeaf",
            "openFile",
        ))
        .build()
        .unwrap()
}
