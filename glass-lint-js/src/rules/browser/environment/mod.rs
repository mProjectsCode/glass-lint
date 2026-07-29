//! Browser environment-property rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals, while
/// unlisted properties and dynamic names are ignored.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("browser.environment")
        .description("Reads browser environment data")
        .category(Category::new("browser/environment").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_read_rooted("navigator.userAgent"))
        .query(QueryDecl::member_read_rooted("navigator.platform"))
        .query(QueryDecl::member_read_rooted("navigator.language"))
        .query(QueryDecl::member_read_rooted("screen.width"))
        .query(QueryDecl::member_read_rooted("screen.height"))
        .query(QueryDecl::member_read_rooted("screen.availWidth"))
        .query(QueryDecl::member_read_rooted("screen.availHeight"))
        .query(QueryDecl::member_read_rooted("screen.colorDepth"))
        .query(QueryDecl::member_read_rooted("screen.pixelDepth"))
        .query(QueryDecl::member_read_rooted("navigator.languages"))
        .query(QueryDecl::member_read_rooted(
            "navigator.hardwareConcurrency",
        ))
        .query(QueryDecl::member_read_rooted("navigator.deviceMemory"))
        .query(QueryDecl::member_read_rooted("navigator.vendor"))
        .query(QueryDecl::member_read_rooted("navigator.cookieEnabled"))
        .query(QueryDecl::member_read_rooted("navigator.maxTouchPoints"))
        .query(QueryDecl::member_read_rooted("navigator.doNotTrack"))
        .query(QueryDecl::member_read_rooted("navigator.webdriver"))
        .query(QueryDecl::member_read_rooted("navigator.pdfViewerEnabled"))
        .query(QueryDecl::member_read_rooted("navigator.onLine"))
        .query(QueryDecl::member_read_rooted(
            "navigator.connection.effectiveType",
        ))
        .query(QueryDecl::member_read_rooted("navigator.connection.rtt"))
        .query(QueryDecl::member_read_rooted(
            "navigator.connection.downlink",
        ))
        .query(QueryDecl::member_read_rooted(
            "navigator.connection.saveData",
        ))
        .build()
        .unwrap()
}
