//! Browser File System Access API rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted directory-picker entry points and operations on directory
/// handles returned by them. Nested file handles and arbitrary object wrappers
/// remain outside this bounded rule.
pub fn rule() -> Rule {
    Rule::builder("browser.filesystem")
        .description("Uses browser file-system access")
        .category("browser/filesystem")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("showDirectoryPicker"))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "getFileHandle",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "getDirectoryHandle",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "removeEntry",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "resolve",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "queryPermission",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "requestPermission",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "entries",
        ))
        .matcher(Matcher::returned_member_call("showDirectoryPicker", "keys"))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "values",
        ))
        .matcher(Matcher::returned_member_call(
            "showDirectoryPicker",
            "isSameEntry",
        ))
        .build()
        .unwrap()
}
