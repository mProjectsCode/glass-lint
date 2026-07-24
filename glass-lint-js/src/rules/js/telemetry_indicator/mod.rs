//! Telemetry SDK and endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/browser")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/node")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("posthog-js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("mixpanel-browser")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/electron")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/react")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/vue")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sentry/nextjs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@opentelemetry/api")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@opentelemetry/sdk-node")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@opentelemetry/sdk-trace-web")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@opentelemetry/exporter-trace-otlp-http")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@segment/analytics-next")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("analytics")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@amplitude/analytics-browser")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@datadog/browser-rum")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@logrocket/react")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("fullstory")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("sentry.io")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("google-analytics.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("app.posthog.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.segment.io")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("browser-intake-datadoghq.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.amplitude.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("logrocket.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
