//! Browser worker and worklet opaque-execution rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

const WORKER_METHODS: &[&str] = &[
    "CSS.animationWorklet.addModule",
    "CSS.layoutWorklet.addModule",
    "CSS.paintWorklet.addModule",
    "navigator.serviceWorker.getRegistration",
    "navigator.serviceWorker.getRegistrations",
    "navigator.serviceWorker.register",
];

/// Detects browser APIs that start or load JavaScript in a worker-like
/// execution context: dedicated/shared workers, service workers, CSS
/// worklets, and worker-side `importScripts`. The launched or loaded code is
/// outside the current JavaScript analysis boundary.
pub fn rule() -> Rule {
    Rule::builder("browser.worker")
        .description("Starts or loads background JavaScript")
        .category(Category::new("browser/concurrency").unwrap())
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .query(EventQuery::constructor_global("Worker"))
        .query(EventQuery::constructor_global("SharedWorker"))
        .query(EventQuery::call_global("importScripts"))
        .queries(
            WORKER_METHODS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .build()
        .unwrap()
}
