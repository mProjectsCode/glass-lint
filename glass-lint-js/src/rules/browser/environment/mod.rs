//! Browser environment-property rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const BROWSER_PROPERTIES: &[&str] = &[
    "navigator.userAgent",
    "navigator.userAgentData",
    "navigator.globalPrivacyControl",
    "navigator.platform",
    "navigator.language",
    "screen.width",
    "screen.height",
    "screen.availWidth",
    "screen.availHeight",
    "screen.colorDepth",
    "screen.pixelDepth",
    "screen.orientation",
    "screen.availLeft",
    "screen.availTop",
    "navigator.languages",
    "navigator.hardwareConcurrency",
    "navigator.deviceMemory",
    "navigator.vendor",
    "navigator.cookieEnabled",
    "navigator.maxTouchPoints",
    "navigator.doNotTrack",
    "navigator.webdriver",
    "navigator.pdfViewerEnabled",
    "navigator.onLine",
    "navigator.connection.effectiveType",
    "navigator.connection.rtt",
    "navigator.connection.downlink",
    "navigator.connection.saveData",
];

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals, while
/// unlisted properties and dynamic names are ignored.
pub fn rule() -> Rule {
    Rule::catalog_builder("browser.environment")
        .description("Reads browser environment data")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .queries(
            BROWSER_PROPERTIES
                .iter()
                .copied()
                .map(EventQuery::member_read_rooted),
        )
        .build()
        .unwrap()
}
