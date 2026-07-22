//! Browser File System Access API rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted directory-picker entry points and operations on directory
/// handles returned by them. Nested file handles and arbitrary object wrappers
/// remain outside this bounded rule.
pub fn rule() -> Rule {
    Rule::builder("browser.filesystem")
        .description("Uses browser file-system access")
        .category("browser/filesystem")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("showDirectoryPicker"))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "getFileHandle",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "getDirectoryHandle",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "removeEntry",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "resolve",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "queryPermission",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "requestPermission",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "entries",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "keys",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "values",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "showDirectoryPicker",
            "isSameEntry",
        ))
        .build()
        .unwrap()
}
