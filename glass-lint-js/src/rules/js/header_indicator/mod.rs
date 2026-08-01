//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

const HEADER_MARKERS: &[&str] = &[
    "User-Agent",
    "user-agent",
    "USER-AGENT",
    "Authorization",
    "authorization",
    "AUTHORIZATION",
    "Cookie",
    "COOKIE",
    "Proxy-Authorization",
    "PROXY-AUTHORIZATION",
    "X-API-Key",
    "x-api-key",
    "Api-Key",
    "api-key",
    "API-KEY",
    "X-Auth-Token",
    "x-auth-token",
    "X-Access-Token",
    "x-access-token",
    "X-Client-Token",
    "x-client-token",
    "X-API-Token",
    "x-api-token",
];

/// Detects string literals containing the configured `Authorization` and
/// `User-Agent` marker substrings. This is an opt-in heuristic indicator: it
/// does not prove that a literal is used as a request header, rejects dynamic
/// values, and intentionally excludes other casing and unrelated lookalike
/// prose. Bounded constant compositions are evaluated by core.
pub fn rule() -> Rule {
    Rule::builder("network.header-indicator")
        .description("References authorization or user-agent headers")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        // Sink-associated coverage proves header names in request option
        // objects; literal matchers below intentionally retain this rule's
        // source-wide heuristic policy.
        .query(
            EventQuery::call_global("fetch")
                .map(|q| {
                    q.with_arg_object_keys(
                        1,
                        [
                            "User-Agent",
                            "user-agent",
                            "Authorization",
                            "authorization",
                            "Cookie",
                            "cookie",
                            "X-API-Key",
                            "x-api-key",
                        ],
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap()
        )
        .queries(HEADER_MARKERS.iter().copied().map(EventQuery::string_contains))
        .build()
        .unwrap()
}
