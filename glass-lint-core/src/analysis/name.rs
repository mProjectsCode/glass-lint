//! Bounded names owned by one semantic artifact.
//!
//! The canonical types live in [`glass_lint_datastructures`]; this module only
//! holds the artifact-level bound.

/// Core bound for one artifact; it matches the default semantic-operation
/// bound while remaining independent of process lifetime and scheduling.
pub(in crate::analysis) const MAX_NAMES: usize = 1 << 20;
