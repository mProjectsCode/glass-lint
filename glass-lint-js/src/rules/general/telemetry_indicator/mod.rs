use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed telemetry
/// SDKs and string literals containing configured telemetry endpoint markers.
/// Module matches use exact module provenance; literal matches are
/// medium-confidence substring heuristics over literals and template quasis,
/// not proof that a request or telemetry event occurs.
pub fn rule() -> Rule {
    Rule::builder("network.telemetry-indicator")
        .label("References telemetry SDKs or endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("@sentry/browser"))
        .matcher(Matcher::import("@sentry/node"))
        .matcher(Matcher::import("posthog-js"))
        .matcher(Matcher::import("mixpanel-browser"))
        .matcher(Matcher::string_literal("sentry.io"))
        .matcher(Matcher::string_literal("google-analytics.com"))
        .build()
        .unwrap()
}
