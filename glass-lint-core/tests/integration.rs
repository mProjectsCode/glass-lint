//! Public integration coverage for the core analysis engine.
//!
//! The test tree is grouped by the boundary it exercises: matcher behavior,
//! query authoring/planning, linter/report behavior, public API invariants,
//! and TypeScript input handling.

#[path = "integration/linter.rs"]
mod linter;
#[path = "integration/matching/mod.rs"]
mod matching;
#[path = "integration/public_surface.rs"]
mod public_surface;
#[path = "integration/query/mod.rs"]
mod query;
#[path = "integration/support.rs"]
mod support;
#[path = "integration/typescript.rs"]
mod typescript;
