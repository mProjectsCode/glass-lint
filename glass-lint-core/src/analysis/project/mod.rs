//! Staged project-linking implementation.
//!
//! The project pass keeps each module's local semantic model isolated, then
//! adds deterministic export identities, graph metadata, and bounded flow
//! projections on top. All linking state is additive: local artifacts are
//! never mutated after construction.
//!
//! Unknown or over-budget links remain conservative so a partial project
//! cannot manufacture cross-file provenance. The link pass resolves exports
//! to a fixed point via SCCs and produces a `ModuleOccurrenceOverlay` for
//! each module's matcher queries.

pub(super) mod model;

mod exports;
mod graph;
pub(super) mod identities;
pub mod projection;
pub(super) mod state;
