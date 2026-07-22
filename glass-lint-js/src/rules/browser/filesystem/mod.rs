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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("showDirectoryPicker")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "getFileHandle")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "getDirectoryHandle")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "removeEntry")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "resolve")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "queryPermission")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "requestPermission")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "entries")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "keys")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "values")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_returned("showDirectoryPicker", "isSameEntry")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
