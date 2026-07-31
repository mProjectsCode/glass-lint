//! Private-network address indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects complete static localhost, private, loopback, link-local,
/// documentation, wildcard, and selected special-use IPv4/IPv6 addresses.
/// Boundary-aware parsing avoids matching prose or address fragments; dynamic
/// and concatenated values remain outside this source-wide indicator.
pub fn rule() -> Rule {
    Rule::builder("network.private-address")
        .description("References private-network addresses")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::string_private_network_address())
        .build()
        .unwrap()
}
