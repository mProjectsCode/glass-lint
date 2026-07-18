//! Browser environment-property rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals, while
/// unlisted properties and dynamic names are ignored.
pub fn rule() -> Rule {
    Rule::builder("browser.environment")
        .description("Reads browser environment data")
        .category("browser/environment")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_read("navigator.userAgent"))
        .matcher(Matcher::rooted_member_read("navigator.platform"))
        .matcher(Matcher::rooted_member_read("navigator.language"))
        .matcher(Matcher::rooted_member_read("screen.width"))
        .matcher(Matcher::rooted_member_read("screen.height"))
        .matcher(Matcher::rooted_member_read("screen.availWidth"))
        .matcher(Matcher::rooted_member_read("screen.availHeight"))
        .matcher(Matcher::rooted_member_read("screen.colorDepth"))
        .matcher(Matcher::rooted_member_read("screen.pixelDepth"))
        .matcher(Matcher::rooted_member_read("navigator.languages"))
        .matcher(Matcher::rooted_member_read("navigator.hardwareConcurrency"))
        .matcher(Matcher::rooted_member_read("navigator.deviceMemory"))
        .matcher(Matcher::rooted_member_read("navigator.vendor"))
        .matcher(Matcher::rooted_member_read("navigator.cookieEnabled"))
        .matcher(Matcher::rooted_member_read("navigator.maxTouchPoints"))
        .matcher(Matcher::rooted_member_read("navigator.doNotTrack"))
        .matcher(Matcher::rooted_member_read("navigator.webdriver"))
        .matcher(Matcher::rooted_member_read("navigator.pdfViewerEnabled"))
        .matcher(Matcher::rooted_member_read("navigator.onLine"))
        .matcher(Matcher::rooted_member_read(
            "navigator.connection.effectiveType",
        ))
        .matcher(Matcher::rooted_member_read("navigator.connection.rtt"))
        .matcher(Matcher::rooted_member_read("navigator.connection.downlink"))
        .matcher(Matcher::rooted_member_read("navigator.connection.saveData"))
        .build()
        .unwrap()
}
