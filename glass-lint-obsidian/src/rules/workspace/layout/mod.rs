//! Obsidian workspace-layout rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted calls to `getLayout`, `changeLayout`, and
/// `requestSaveLayout` on `app.workspace`. Provenance follows `this.app`,
/// workspace aliases, static computed properties, source-ordered alias
/// reassignment, and lexical shadowing. Dynamic or unlisted members, local
/// lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.layout")
        .description("Reads or writes workspace layout")
        .category(Category::new("workspace").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_call_rooted("app.workspace.getLayout"))
        .query(QueryDecl::member_call_rooted("app.workspace.changeLayout"))
        .query(QueryDecl::member_call_rooted("app.workspace.requestSaveLayout"))
        .build()
        .unwrap()
}
