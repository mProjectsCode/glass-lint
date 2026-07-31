//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects string literals containing the configured `Authorization` and
/// `User-Agent` marker substrings. This is an opt-in heuristic indicator: it
/// does not prove that a literal is used as a request header, does not parse
/// computed or concatenated values, and intentionally excludes other casing
/// and unrelated lookalike prose.
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
        .query(EventQuery::string_contains("User-Agent"))
        .query(EventQuery::string_contains("user-agent"))
        .query(EventQuery::string_contains("USER-AGENT"))
        .query(EventQuery::string_contains("Authorization"))
        .query(EventQuery::string_contains("authorization"))
        .query(EventQuery::string_contains("AUTHORIZATION"))
        .query(EventQuery::string_contains("Cookie"))
        .query(EventQuery::string_contains("COOKIE"))
        .query(EventQuery::string_contains("Proxy-Authorization"))
        .query(EventQuery::string_contains("PROXY-AUTHORIZATION"))
        .query(EventQuery::string_contains("X-API-Key"))
        .query(EventQuery::string_contains("x-api-key"))
        .query(EventQuery::string_contains("Api-Key"))
        .query(EventQuery::string_contains("api-key"))
        .query(EventQuery::string_contains("API-KEY"))
        .query(EventQuery::string_contains("X-Auth-Token"))
        .query(EventQuery::string_contains("x-auth-token"))
        .query(EventQuery::string_contains("X-Access-Token"))
        .query(EventQuery::string_contains("x-access-token"))
        .query(EventQuery::string_contains("X-Client-Token"))
        .query(EventQuery::string_contains("x-client-token"))
        .query(EventQuery::string_contains("X-API-Token"))
        .query(EventQuery::string_contains("x-api-token"))
        .build()
        .unwrap()
}
