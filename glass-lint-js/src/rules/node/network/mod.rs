//! Node HTTP-module rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the configured Node
/// network modules and exact client packages. It reports the module load
/// itself, not later API use, and relies on module provenance so similar names
/// and shadowed `require` bindings are excluded.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("node.network")
        .description("Uses Node HTTP modules")
        .category(Category::new("node/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::import_exact("http"))
        .query(QueryDecl::import_exact("https"))
        .query(QueryDecl::import_exact("node:http"))
        .query(QueryDecl::import_exact("node:https"))
        .query(QueryDecl::import_exact("http2"))
        .query(QueryDecl::import_exact("node:http2"))
        .query(QueryDecl::import_exact("net"))
        .query(QueryDecl::import_exact("node:net"))
        .query(QueryDecl::import_exact("tls"))
        .query(QueryDecl::import_exact("node:tls"))
        .query(QueryDecl::import_exact("dgram"))
        .query(QueryDecl::import_exact("node:dgram"))
        .query(QueryDecl::import_exact("dns"))
        .query(QueryDecl::import_exact("node:dns"))
        .query(QueryDecl::import_exact("dns/promises"))
        .query(QueryDecl::import_exact("node:dns/promises"))
        .query(QueryDecl::import_package("undici"))
        .query(QueryDecl::import_package("axios"))
        .query(QueryDecl::import_package("node-fetch"))
        .query(QueryDecl::import_package("got"))
        .query(QueryDecl::import_package("superagent"))
        .query(QueryDecl::import_package("ws"))
        .query(QueryDecl::import_package("cross-fetch"))
        .query(QueryDecl::import_package("ky"))
        .query(QueryDecl::import_package("graphql-request"))
        .query(QueryDecl::import_package("request"))
        .query(QueryDecl::import_package("needle"))
        .query(QueryDecl::import_package("@grpc/grpc-js"))
        .query(QueryDecl::import_package("@apollo/client"))
        .query(QueryDecl::import_package("graphql"))
        .query(QueryDecl::import_package("@elastic/elasticsearch"))
        .query(QueryDecl::import_package("fetch-retry"))
        .query(QueryDecl::import_package("fetch-blob"))
        .query(QueryDecl::import_package("form-data"))
        .query(QueryDecl::import_package("http-proxy"))
        .query(QueryDecl::import_package("http-proxy-agent"))
        .query(QueryDecl::import_package("https-proxy-agent"))
        .query(QueryDecl::import_package("socks-proxy-agent"))
        .query(QueryDecl::import_package("@whatwg-node/fetch"))
        .build()
        .unwrap()
}
