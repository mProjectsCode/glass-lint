//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const TELEMETRY_PACKAGES: &[&str] = &[
    "@sentry/browser",
    "@sentry/node",
    "posthog-js",
    "mixpanel-browser",
    "@sentry/electron",
    "@sentry/react",
    "@sentry/vue",
    "@sentry/nextjs",
    "@opentelemetry/api",
    "@opentelemetry/sdk-node",
    "@opentelemetry/sdk-trace-web",
    "@opentelemetry/exporter-trace-otlp-http",
    "@segment/analytics-next",
    "analytics",
    "@amplitude/analytics-browser",
    "@datadog/browser-rum",
    "@logrocket/react",
    "fullstory",
];

const TELEMETRY_ENDPOINTS: &[&str] = &[
    "sentry.io",
    "google-analytics.com",
    "app.posthog.com",
    "api.segment.io",
    "browser-intake-datadoghq.com",
    "api.amplitude.com",
    "logrocket.com",
];

/// Detects static ESM or unshadowed CommonJS loads of the listed telemetry
/// SDKs and string literals containing configured telemetry endpoint markers.
/// Module matches use exact module provenance; literal matches are
/// medium-confidence substring heuristics over literals, template quasis, and
/// bounded constant compositions, not proof that a request or telemetry event
/// occurs. This is intentionally a low-confidence dependency/literal
/// indicator, not an operation witness.
pub fn rule() -> Rule {
    Rule::catalog_builder("network.telemetry-indicator")
        .description("References telemetry SDKs or endpoints")
        .severity(Severity::Info)
        .confidence(Confidence::Low)
        .queries(
            TELEMETRY_PACKAGES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .queries(
            TELEMETRY_ENDPOINTS
                .iter()
                .copied()
                .map(EventQuery::string_contains),
        )
        .build()
        .unwrap()
}
