//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed telemetry
/// SDKs and string literals containing configured telemetry endpoint markers.
/// Module matches use exact module provenance; literal matches are
/// medium-confidence substring heuristics over literals and template quasis,
/// not proof that a request or telemetry event occurs.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("network.telemetry-indicator")
        .description("References telemetry SDKs or endpoints")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::import_package("@sentry/browser"))
        .query(EventQuery::import_package("@sentry/node"))
        .query(EventQuery::import_package("posthog-js"))
        .query(EventQuery::import_package("mixpanel-browser"))
        .query(EventQuery::import_package("@sentry/electron"))
        .query(EventQuery::import_package("@sentry/react"))
        .query(EventQuery::import_package("@sentry/vue"))
        .query(EventQuery::import_package("@sentry/nextjs"))
        .query(EventQuery::import_package("@opentelemetry/api"))
        .query(EventQuery::import_package("@opentelemetry/sdk-node"))
        .query(EventQuery::import_package("@opentelemetry/sdk-trace-web"))
        .query(EventQuery::import_package(
            "@opentelemetry/exporter-trace-otlp-http",
        ))
        .query(EventQuery::import_package("@segment/analytics-next"))
        .query(EventQuery::import_package("analytics"))
        .query(EventQuery::import_package("@amplitude/analytics-browser"))
        .query(EventQuery::import_package("@datadog/browser-rum"))
        .query(EventQuery::import_package("@logrocket/react"))
        .query(EventQuery::import_package("fullstory"))
        .query(EventQuery::string_contains("sentry.io"))
        .query(EventQuery::string_contains("google-analytics.com"))
        .query(EventQuery::string_contains("app.posthog.com"))
        .query(EventQuery::string_contains("api.segment.io"))
        .query(EventQuery::string_contains("browser-intake-datadoghq.com"))
        .query(EventQuery::string_contains("api.amplitude.com"))
        .query(EventQuery::string_contains("logrocket.com"))
        .build()
        .unwrap()
}
