//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed telemetry
/// SDKs and string literals containing configured telemetry endpoint markers.
/// Module matches use exact module provenance; literal matches are
/// medium-confidence substring heuristics over literals and template quasis,
/// not proof that a request or telemetry event occurs.
pub fn rule() -> Rule {
    Rule::builder("network.telemetry-indicator")
        .description("References telemetry SDKs or endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("@sentry/browser"))
        .matcher(Matcher::import("@sentry/node"))
        .matcher(Matcher::import("posthog-js"))
        .matcher(Matcher::import("mixpanel-browser"))
        .matcher(Matcher::import("@sentry/electron"))
        .matcher(Matcher::import("@sentry/react"))
        .matcher(Matcher::import("@sentry/vue"))
        .matcher(Matcher::import("@sentry/nextjs"))
        .matcher(Matcher::import("@opentelemetry/api"))
        .matcher(Matcher::import("@opentelemetry/sdk-node"))
        .matcher(Matcher::import("@opentelemetry/sdk-trace-web"))
        .matcher(Matcher::import("@opentelemetry/exporter-trace-otlp-http"))
        .matcher(Matcher::import("@segment/analytics-next"))
        .matcher(Matcher::import("analytics"))
        .matcher(Matcher::import("@amplitude/analytics-browser"))
        .matcher(Matcher::import("@datadog/browser-rum"))
        .matcher(Matcher::import("@logrocket/react"))
        .matcher(Matcher::import("fullstory"))
        .matcher(Matcher::string_contains("sentry.io"))
        .matcher(Matcher::string_contains("google-analytics.com"))
        .matcher(Matcher::string_contains("app.posthog.com"))
        .matcher(Matcher::string_contains("api.segment.io"))
        .matcher(Matcher::string_contains("browser-intake-datadoghq.com"))
        .matcher(Matcher::string_contains("api.amplitude.com"))
        .matcher(Matcher::string_contains("logrocket.com"))
        .build()
        .unwrap()
}
