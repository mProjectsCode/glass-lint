//! Browser environment-property rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals, while
/// unlisted properties and dynamic names are ignored.
pub fn rule() -> Rule {
    Rule::builder("browser.environment")
        .description("Reads browser environment data")
        .category("browser/environment")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::rooted_member_read("navigator.userAgent"))
        .declaration(MatcherDecl::rooted_member_read("navigator.platform"))
        .declaration(MatcherDecl::rooted_member_read("navigator.language"))
        .declaration(MatcherDecl::rooted_member_read("screen.width"))
        .declaration(MatcherDecl::rooted_member_read("screen.height"))
        .declaration(MatcherDecl::rooted_member_read("screen.availWidth"))
        .declaration(MatcherDecl::rooted_member_read("screen.availHeight"))
        .declaration(MatcherDecl::rooted_member_read("screen.colorDepth"))
        .declaration(MatcherDecl::rooted_member_read("screen.pixelDepth"))
        .declaration(MatcherDecl::rooted_member_read("navigator.languages"))
        .declaration(MatcherDecl::rooted_member_read(
            "navigator.hardwareConcurrency",
        ))
        .declaration(MatcherDecl::rooted_member_read("navigator.deviceMemory"))
        .declaration(MatcherDecl::rooted_member_read("navigator.vendor"))
        .declaration(MatcherDecl::rooted_member_read("navigator.cookieEnabled"))
        .declaration(MatcherDecl::rooted_member_read("navigator.maxTouchPoints"))
        .declaration(MatcherDecl::rooted_member_read("navigator.doNotTrack"))
        .declaration(MatcherDecl::rooted_member_read("navigator.webdriver"))
        .declaration(MatcherDecl::rooted_member_read(
            "navigator.pdfViewerEnabled",
        ))
        .declaration(MatcherDecl::rooted_member_read("navigator.onLine"))
        .declaration(MatcherDecl::rooted_member_read(
            "navigator.connection.effectiveType",
        ))
        .declaration(MatcherDecl::rooted_member_read("navigator.connection.rtt"))
        .declaration(MatcherDecl::rooted_member_read(
            "navigator.connection.downlink",
        ))
        .declaration(MatcherDecl::rooted_member_read(
            "navigator.connection.saveData",
        ))
        .build()
        .unwrap()
}
