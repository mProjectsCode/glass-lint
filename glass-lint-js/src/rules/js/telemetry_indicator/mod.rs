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
        .matcher(Matcher::package_import("@sentry/browser"))
        .matcher(Matcher::package_import("@sentry/node"))
        .matcher(Matcher::package_import("posthog-js"))
        .matcher(Matcher::package_import("mixpanel-browser"))
        .matcher(Matcher::package_import("@sentry/electron"))
        .matcher(Matcher::package_import("@sentry/react"))
        .matcher(Matcher::package_import("@sentry/vue"))
        .matcher(Matcher::package_import("@sentry/nextjs"))
        .matcher(Matcher::package_import("@opentelemetry/api"))
        .matcher(Matcher::package_import("@opentelemetry/sdk-node"))
        .matcher(Matcher::package_import("@opentelemetry/sdk-trace-web"))
        .matcher(Matcher::package_import(
            "@opentelemetry/exporter-trace-otlp-http",
        ))
        .matcher(Matcher::package_import("@segment/analytics-next"))
        .matcher(Matcher::package_import("analytics"))
        .matcher(Matcher::package_import("@amplitude/analytics-browser"))
        .matcher(Matcher::package_import("@datadog/browser-rum"))
        .matcher(Matcher::package_import("@logrocket/react"))
        .matcher(Matcher::package_import("fullstory"))
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
