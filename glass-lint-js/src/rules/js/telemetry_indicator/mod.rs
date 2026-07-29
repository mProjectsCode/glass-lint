//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::import_package("@sentry/browser"))
        .query(QueryDecl::import_package("@sentry/node"))
        .query(QueryDecl::import_package("posthog-js"))
        .query(QueryDecl::import_package("mixpanel-browser"))
        .query(QueryDecl::import_package("@sentry/electron"))
        .query(QueryDecl::import_package("@sentry/react"))
        .query(QueryDecl::import_package("@sentry/vue"))
        .query(QueryDecl::import_package("@sentry/nextjs"))
        .query(QueryDecl::import_package("@opentelemetry/api"))
        .query(QueryDecl::import_package("@opentelemetry/sdk-node"))
        .query(QueryDecl::import_package("@opentelemetry/sdk-trace-web"))
        .query(QueryDecl::import_package(
            "@opentelemetry/exporter-trace-otlp-http",
        ))
        .query(QueryDecl::import_package("@segment/analytics-next"))
        .query(QueryDecl::import_package("analytics"))
        .query(QueryDecl::import_package("@amplitude/analytics-browser"))
        .query(QueryDecl::import_package("@datadog/browser-rum"))
        .query(QueryDecl::import_package("@logrocket/react"))
        .query(QueryDecl::import_package("fullstory"))
        .query(QueryDecl::string_contains("sentry.io"))
        .query(QueryDecl::string_contains("google-analytics.com"))
        .query(QueryDecl::string_contains("app.posthog.com"))
        .query(QueryDecl::string_contains("api.segment.io"))
        .query(QueryDecl::string_contains("browser-intake-datadoghq.com"))
        .query(QueryDecl::string_contains("api.amplitude.com"))
        .query(QueryDecl::string_contains("logrocket.com"))
        .build()
        .unwrap()
}
