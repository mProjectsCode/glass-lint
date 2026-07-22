//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::package_import("@sentry/browser"))
        .declaration(MatcherDecl::package_import("@sentry/node"))
        .declaration(MatcherDecl::package_import("posthog-js"))
        .declaration(MatcherDecl::package_import("mixpanel-browser"))
        .declaration(MatcherDecl::package_import("@sentry/electron"))
        .declaration(MatcherDecl::package_import("@sentry/react"))
        .declaration(MatcherDecl::package_import("@sentry/vue"))
        .declaration(MatcherDecl::package_import("@sentry/nextjs"))
        .declaration(MatcherDecl::package_import("@opentelemetry/api"))
        .declaration(MatcherDecl::package_import("@opentelemetry/sdk-node"))
        .declaration(MatcherDecl::package_import("@opentelemetry/sdk-trace-web"))
        .declaration(MatcherDecl::package_import(
            "@opentelemetry/exporter-trace-otlp-http",
        ))
        .declaration(MatcherDecl::package_import("@segment/analytics-next"))
        .declaration(MatcherDecl::package_import("analytics"))
        .declaration(MatcherDecl::package_import("@amplitude/analytics-browser"))
        .declaration(MatcherDecl::package_import("@datadog/browser-rum"))
        .declaration(MatcherDecl::package_import("@logrocket/react"))
        .declaration(MatcherDecl::package_import("fullstory"))
        .declaration(MatcherDecl::string_contains("sentry.io"))
        .declaration(MatcherDecl::string_contains("google-analytics.com"))
        .declaration(MatcherDecl::string_contains("app.posthog.com"))
        .declaration(MatcherDecl::string_contains("api.segment.io"))
        .declaration(MatcherDecl::string_contains("browser-intake-datadoghq.com"))
        .declaration(MatcherDecl::string_contains("api.amplitude.com"))
        .declaration(MatcherDecl::string_contains("logrocket.com"))
        .build()
        .unwrap()
}
