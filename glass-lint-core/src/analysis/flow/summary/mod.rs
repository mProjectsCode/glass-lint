//! Function summaries projected from the canonical fact stream.
//!
//! A summary is keyed only by `FunctionId`. Parameter paths and argument
//! projections keep destructuring precise, while the fixed point joins helper
//! calls (including recursive and mutually recursive helpers) without walking
//! AST bodies again.
//!
//! Summaries are monotone and conservative: unsupported reassignment,
//! dynamic arguments, missing paths, or incompatible invocations do not create
//! a projected sink. Recursive propagation stops at a fixed point or its
//! explicit round bound.
//!
//! Path storage uses one shared [`glass_lint_datastructures::ParentPathStore`]
//! for the summary overlay. A [`SummaryPathId`] is either a frozen [`PathId`]
//! reference (no copying) or an overlay node created during a join.  The
//! overlay is bounded by [`MAX_OVERLAY_NODES`]; exhaustion fails closed.

mod parameter;
mod sink;
pub(super) mod store;
mod summaries;

pub(super) use store::SummaryPathStore;
pub(super) use summaries::FunctionSummaries;

const MAX_SUMMARY_ROUNDS: usize = 64;
