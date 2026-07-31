//! Browser File System Access API rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, QueryDecl, Rule, Severity};

/// Detects rooted directory-picker entry points and operations on directory
/// handles returned by them. Nested file handles and writable streams are
/// followed through bounded returned-object paths; arbitrary wrappers remain
/// outside this rule.
pub fn rule() -> Rule {
    Rule::builder("browser.filesystem")
        .description("Uses browser file-system access")
        .category(Category::new("browser/filesystem").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("showDirectoryPicker"))
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
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle",
            "getFile",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle",
            "createWritable",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle.createWritable",
            "write",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle.createWritable",
            "seek",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle.createWritable",
            "truncate",
        ))
        .query(QueryDecl::member_call_returned(
            "showDirectoryPicker.getFileHandle.createWritable",
            "close",
        ))
        .build()
        .unwrap()
}
