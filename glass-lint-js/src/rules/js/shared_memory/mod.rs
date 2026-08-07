//! Shared-memory concurrency rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const ATOMICS_METHODS: &[&str] = &[
    "Atomics.add",
    "Atomics.and",
    "Atomics.compareExchange",
    "Atomics.exchange",
    "Atomics.isLockFree",
    "Atomics.load",
    "Atomics.notify",
    "Atomics.or",
    "Atomics.store",
    "Atomics.sub",
    "Atomics.wait",
    "Atomics.waitAsync",
    "Atomics.wake",
    "Atomics.xor",
];

/// Detects shared-memory primitives that coordinate state across workers or
/// agents. This is an informational capability signal; it does not claim that
/// the program creates a worker or that a particular shared buffer is used.
pub fn rule() -> Rule {
    Rule::builder("concurrency.shared-memory")
        .description("Uses shared-memory concurrency APIs")
        .confidence(Confidence::High)
        .severity(Severity::Info)
        .query(EventQuery::constructor_global("SharedArrayBuffer"))
        .queries(
            ATOMICS_METHODS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .build()
        .unwrap()
}
