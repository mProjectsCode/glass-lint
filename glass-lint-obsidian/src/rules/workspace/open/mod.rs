//! Obsidian workspace-open rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, QueryDecl, Rule, Severity};

/// Detects rooted calls to `app.workspace.openLinkText` and
/// `app.workspace.getLeaf.openFile`. Provenance follows `this.app`, workspace
/// aliases, static computed properties, source-ordered alias reassignment,
/// and lexical shadowing. Dynamic or unlisted members, local lookalikes, and
/// call arguments are not analyzed. Returned leaves are followed through
/// simple aliases while reassignment and shadowing remain fail-closed.
pub fn rule() -> Rule {
    Rule::builder("workspace.open")
        .description("Opens files through the workspace")
        .category(Category::new("workspace").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("app.workspace.openLinkText"))
        .query(EventQuery::member_call_rooted(
            "app.workspace.getLeaf.openFile",
        ))
        .query(QueryDecl::member_call_returned(
            "app.workspace.getLeaf",
            "openFile",
        ))
        .query(QueryDecl::member_call_returned(
            "app.workspace.getLeafById",
            "openFile",
        ))
        .query(QueryDecl::member_call_returned(
            "app.workspace.getLeftLeaf",
            "openFile",
        ))
        .query(QueryDecl::member_call_returned(
            "app.workspace.getRightLeaf",
            "openFile",
        ))
        .query(QueryDecl::member_call_returned(
            "app.workspace.ensureSideLeaf",
            "openFile",
        ))
        .build()
        .unwrap()
}
