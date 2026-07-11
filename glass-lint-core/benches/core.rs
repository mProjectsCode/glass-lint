//! Lightweight dependency-free core benchmark target.
//!
//! Run with `cargo bench -p glass-lint-core`.  It intentionally exercises the
//! semantic build and report path with representative source shapes; the
//! printed timings are suitable for local before/after comparisons without
//! making the workspace depend on a benchmark framework.

use std::time::Instant;

use glass_lint_core::{
    Linter, RuleCatalog,
    rules::{Confidence, Matcher, Rule, Severity},
};

fn main() {
    let rule = Rule::builder("benchmark.fetch")
        .label("benchmark")
        .category("benchmark")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .matcher(Matcher::rooted_member_call("client.request"))
        .build()
        .expect("benchmark rule is valid");
    let linter = Linter::new(RuleCatalog::new("bench", vec![rule]).expect("catalog is valid"));
    let cases = [
        ("direct", "fetch('/x'); client.request({ url: '/x' });".to_string()),
        ("minified", "const a=fetch,b=client.request;for(let i=0;i<100;i++)a('/'+i);".to_string()),
        ("alias-heavy", (0..100).map(|i| format!("const a{i}=fetch;")).collect::<String>() + "a99('/x');"),
        ("flow-heavy", (0..100).map(|_| "const x=document.createElement('script');x.src=url;document.head.appendChild(x);").collect()),
        (
            "hostile-depth",
            format!("{}fetch('/x'){};", "(".repeat(128), ")".repeat(128)),
        ),
    ];
    for (name, source) in cases {
        let start = Instant::now();
        let report = linter.lint(&source, name);
        let elapsed = start.elapsed();
        println!(
            "{name:14} {:>8.3} ms  findings={} diagnostics={} bytes={}",
            elapsed.as_secs_f64() * 1_000.0,
            report.findings.len(),
            report.parse_diagnostics.len(),
            source.len()
        );
    }
}
