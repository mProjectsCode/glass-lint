//! WebAssembly opaque-execution rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const WEBASSEMBLY_METHODS: &[&str] = &[
    "WebAssembly.compile",
    "WebAssembly.compileStreaming",
    "WebAssembly.instantiate",
    "WebAssembly.instantiateStreaming",
    "WebAssembly.validate",
];

const WEBASSEMBLY_CONSTRUCTORS: &[&str] = &[
    "WebAssembly.CompileError",
    "WebAssembly.Exception",
    "WebAssembly.Global",
    "WebAssembly.Instance",
    "WebAssembly.LinkError",
    "WebAssembly.Memory",
    "WebAssembly.Module",
    "WebAssembly.RuntimeError",
    "WebAssembly.Table",
    "WebAssembly.Tag",
];

/// Detects WebAssembly compilation, instantiation, validation, and the
/// standard WebAssembly constructors. The call site is
/// proven through the configured WebAssembly global object, but code compiled
/// or executed by WebAssembly is outside the JavaScript analysis boundary.
pub fn rule() -> Rule {
    Rule::catalog_builder("dynamic-code.webassembly")
        .description("Compiles or executes WebAssembly")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .queries(
            WEBASSEMBLY_METHODS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .queries(
            WEBASSEMBLY_CONSTRUCTORS
                .iter()
                .copied()
                .map(EventQuery::constructor_rooted),
        )
        .build()
        .unwrap()
}
