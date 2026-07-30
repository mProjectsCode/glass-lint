//! Browser environment-property rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

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
        .query(EventQuery::member_read_rooted("navigator.userAgent"))
        .query(EventQuery::member_read_rooted("navigator.platform"))
        .query(EventQuery::member_read_rooted("navigator.language"))
        .query(EventQuery::member_read_rooted("screen.width"))
        .query(EventQuery::member_read_rooted("screen.height"))
        .query(EventQuery::member_read_rooted("screen.availWidth"))
        .query(EventQuery::member_read_rooted("screen.availHeight"))
        .query(EventQuery::member_read_rooted("screen.colorDepth"))
        .query(EventQuery::member_read_rooted("screen.pixelDepth"))
        .query(EventQuery::member_read_rooted("navigator.languages"))
        .query(EventQuery::member_read_rooted(
            "navigator.hardwareConcurrency",
        ))
        .query(EventQuery::member_read_rooted("navigator.deviceMemory"))
        .query(EventQuery::member_read_rooted("navigator.vendor"))
        .query(EventQuery::member_read_rooted("navigator.cookieEnabled"))
        .query(EventQuery::member_read_rooted("navigator.maxTouchPoints"))
        .query(EventQuery::member_read_rooted("navigator.doNotTrack"))
        .query(EventQuery::member_read_rooted("navigator.webdriver"))
        .query(EventQuery::member_read_rooted("navigator.pdfViewerEnabled"))
        .query(EventQuery::member_read_rooted("navigator.onLine"))
        .query(EventQuery::member_read_rooted(
            "navigator.connection.effectiveType",
        ))
        .query(EventQuery::member_read_rooted("navigator.connection.rtt"))
        .query(EventQuery::member_read_rooted(
            "navigator.connection.downlink",
        ))
        .query(EventQuery::member_read_rooted(
            "navigator.connection.saveData",
        ))
        .build()
        .unwrap()
}
