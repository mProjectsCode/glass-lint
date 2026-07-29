//! Browser File System Access API rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted directory-picker entry points and operations on directory
/// handles returned by them. Nested file handles and arbitrary object wrappers
/// remain outside this bounded rule.
pub fn rule() -> Rule {
    Rule::builder("browser.filesystem")
        .description("Uses browser file-system access")
        .category(Category::new("browser/filesystem").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("showDirectoryPicker"))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "getFileHandle",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "getDirectoryHandle",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "removeEntry",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "resolve",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "queryPermission",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "requestPermission",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "entries",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "keys",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "values",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker",
            "isSameEntry",
        ))
        .build()
        .unwrap()
}
