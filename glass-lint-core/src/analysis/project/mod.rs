//! Staged project-linking implementation.
//!
//! The project pass keeps each module's local semantic model isolated, then
//! adds deterministic export identities, graph metadata, and bounded flow
//! projections on top. Unknown or over-budget links remain conservative so a
//! partial project cannot manufacture cross-file provenance.

mod exports;
mod graph;
pub(super) mod identities;
pub mod projection;
pub(super) mod state;
