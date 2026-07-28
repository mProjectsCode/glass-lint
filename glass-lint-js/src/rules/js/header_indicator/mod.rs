//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::call_global("fetch")
                .with_arg_object_keys(
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
                ))
        .query(QueryDecl::string_contains("User-Agent"))
        .query(QueryDecl::string_contains("user-agent"))
        .query(QueryDecl::string_contains("USER-AGENT"))
        .query(QueryDecl::string_contains("Authorization"))
        .query(QueryDecl::string_contains("authorization"))
        .query(QueryDecl::string_contains("AUTHORIZATION"))
        .query(QueryDecl::string_contains("Cookie"))
        .query(QueryDecl::string_contains("COOKIE"))
        .query(QueryDecl::string_contains("Set-Cookie"))
        .query(QueryDecl::string_contains("SET-COOKIE"))
        .query(QueryDecl::string_contains("Proxy-Authorization"))
        .query(QueryDecl::string_contains("PROXY-AUTHORIZATION"))
        .query(QueryDecl::string_contains("X-API-Key"))
        .query(QueryDecl::string_contains("x-api-key"))
        .query(QueryDecl::string_contains("Api-Key"))
        .query(QueryDecl::string_contains("api-key"))
        .query(QueryDecl::string_contains("API-KEY"))
        .query(QueryDecl::string_contains("X-Auth-Token"))
        .query(QueryDecl::string_contains("x-auth-token"))
        .query(QueryDecl::string_contains("X-Access-Token"))
        .query(QueryDecl::string_contains("x-access-token"))
        .query(QueryDecl::string_contains("X-Client-Token"))
        .query(QueryDecl::string_contains("x-client-token"))
        .query(QueryDecl::string_contains("X-API-Token"))
        .query(QueryDecl::string_contains("x-api-token"))
        .build()
        .unwrap()
}
