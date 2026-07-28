//! Private-network address indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects string literals containing the configured localhost, loopback,
/// wildcard, and HTTP(S) `10.*`/`192.168.*` address markers. It is a
/// medium-confidence literal heuristic rather than URL or IP parsing: it does
/// not prove network use, expand private ranges, or match partial,
/// concatenated, or dynamic values.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    let mut builder = Rule::builder("network.private-address")
        .description("References private-network addresses")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::string_contains("localhost"))
        .query(QueryDecl::string_contains("127.0.0.1"))
        .query(QueryDecl::string_contains("http://127."))
        .query(QueryDecl::string_contains("https://127."))
        .query(QueryDecl::string_contains("0.0.0.0"))
        .query(QueryDecl::string_contains("http://192.168."))
        .query(QueryDecl::string_contains("https://192.168."))
        .query(QueryDecl::string_contains("http://10."))
        .query(QueryDecl::string_contains("https://10."))
        .query(QueryDecl::string_contains("http://172.16."))
        .query(QueryDecl::string_contains("https://172.16."))
        .query(QueryDecl::string_contains("http://169.254."))
        .query(QueryDecl::string_contains("https://169.254."))
        .query(QueryDecl::string_contains("::1"))
        .query(QueryDecl::string_contains("fc00:"))
        .query(QueryDecl::string_contains("fd00:"))
        .query(QueryDecl::string_contains("fe80:"));

    for octet in 17..=31 {
        for scheme in ["http://", "https://"] {
            builder = builder.query(QueryDecl::string_contains(format!("{scheme}172.{octet}.")));
        }
    }

    builder.build().unwrap()
}
