//! Bounded semantic flow projection over the immutable fact stream.
//!
//! Local effects and indexes are built once from facts during lowering;
//! matcher-specific projection follows only proven identities and bounded
//! state. The cross-module overlay composes per-function summaries without
//! re-traversing syntax or retaining caller state.
//!
//! Projection is bounded by `AnalysisLimits::flow_operations` and records
//! exhaustion as an `IncompleteReason::BudgetExhausted` status entry rather
//! than synthesizing partial flow state.

pub(super) mod cross;
pub mod effect;
pub(super) mod index;
pub(super) mod matcher;
pub(super) mod plan;
pub(super) mod projector;
pub(super) mod requirements;
pub(super) mod state;
pub(super) mod summary;
pub(super) mod table;
