//! Browser environment-property rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals, while
/// unlisted properties and dynamic names are ignored.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("browser.environment")
        .description("Reads browser environment data")
        .category("browser/environment")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.userAgent")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.platform")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.language")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.width")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.height")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.availWidth")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.availHeight")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.colorDepth")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("screen.pixelDepth")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.languages")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.hardwareConcurrency")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.deviceMemory")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.vendor")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.cookieEnabled")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.maxTouchPoints")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.doNotTrack")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.webdriver")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.pdfViewerEnabled")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.onLine")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.connection.effectiveType")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.connection.rtt")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.connection.downlink")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("navigator.connection.saveData")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
