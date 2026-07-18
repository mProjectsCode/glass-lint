//! Browser environment-property rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects direct reads of a small set of browser environment properties.
/// Rooted matchers preserve identity for configured browser globals. Bare
/// `screen` reads remain heuristic because the global may be exposed through
/// host-specific aliases, while unlisted properties and dynamic names are
/// ignored.
pub fn rule() -> Rule {
    Rule::builder("browser.environment")
        .description("Reads browser environment data")
        .category("browser/environment")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_read("navigator.userAgent"))
        .matcher(Matcher::rooted_member_read("navigator.platform"))
        .matcher(Matcher::rooted_member_read("navigator.language"))
        .matcher(Matcher::heuristic_member_read("screen.width"))
        .matcher(Matcher::heuristic_member_read("screen.height"))
        .matcher(Matcher::heuristic_member_read("screen.availWidth"))
        .matcher(Matcher::heuristic_member_read("screen.availHeight"))
        .matcher(Matcher::heuristic_member_read("screen.colorDepth"))
        .matcher(Matcher::heuristic_member_read("screen.pixelDepth"))
        .matcher(Matcher::rooted_member_read("window.screen.width"))
        .matcher(Matcher::rooted_member_read("window.screen.height"))
        .matcher(Matcher::rooted_member_read("window.screen.availWidth"))
        .matcher(Matcher::rooted_member_read("window.screen.availHeight"))
        .matcher(Matcher::rooted_member_read("window.screen.colorDepth"))
        .matcher(Matcher::rooted_member_read("window.screen.pixelDepth"))
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
        .matcher(Matcher::rooted_member_read("window.navigator.userAgent"))
        .matcher(Matcher::rooted_member_read("window.navigator.platform"))
        .matcher(Matcher::rooted_member_read("window.navigator.language"))
        .matcher(Matcher::rooted_member_read("window.navigator.languages"))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.hardwareConcurrency",
        ))
        .matcher(Matcher::rooted_member_read("window.navigator.deviceMemory"))
        .matcher(Matcher::rooted_member_read("window.navigator.vendor"))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.cookieEnabled",
        ))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.maxTouchPoints",
        ))
        .matcher(Matcher::rooted_member_read("window.navigator.doNotTrack"))
        .matcher(Matcher::rooted_member_read("window.navigator.webdriver"))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.pdfViewerEnabled",
        ))
        .matcher(Matcher::rooted_member_read("window.navigator.onLine"))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.connection.effectiveType",
        ))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.connection.rtt",
        ))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.connection.downlink",
        ))
        .matcher(Matcher::rooted_member_read(
            "window.navigator.connection.saveData",
        ))
        .matcher(Matcher::rooted_member_read("self.navigator.userAgent"))
        .matcher(Matcher::rooted_member_read("self.navigator.platform"))
        .matcher(Matcher::rooted_member_read("self.navigator.language"))
        .matcher(Matcher::rooted_member_read("self.navigator.languages"))
        .matcher(Matcher::rooted_member_read(
            "self.navigator.hardwareConcurrency",
        ))
        .matcher(Matcher::rooted_member_read("self.navigator.deviceMemory"))
        .matcher(Matcher::rooted_member_read("self.navigator.vendor"))
        .matcher(Matcher::rooted_member_read("self.navigator.cookieEnabled"))
        .matcher(Matcher::rooted_member_read("self.navigator.maxTouchPoints"))
        .matcher(Matcher::rooted_member_read("self.navigator.doNotTrack"))
        .matcher(Matcher::rooted_member_read("self.navigator.webdriver"))
        .matcher(Matcher::rooted_member_read(
            "self.navigator.pdfViewerEnabled",
        ))
        .matcher(Matcher::rooted_member_read("self.navigator.onLine"))
        .matcher(Matcher::rooted_member_read(
            "self.navigator.connection.effectiveType",
        ))
        .matcher(Matcher::rooted_member_read("self.navigator.connection.rtt"))
        .matcher(Matcher::rooted_member_read(
            "self.navigator.connection.downlink",
        ))
        .matcher(Matcher::rooted_member_read(
            "self.navigator.connection.saveData",
        ))
        .build()
        .unwrap()
}
