//! Bounded names owned by one semantic artifact.
//!
//! The canonical types live in [`glass_lint_datastructures`]; this module only
//! holds the artifact-level bound.

/// Core bound for one artifact; the value deliberately matches
/// [`glass_lint_datastructures::DEFAULT_MAX_NAMES`] and the default
/// semantic-operation bound while remaining independent of process lifetime
/// and scheduling. The pin is asserted at compile time because the name bound
/// affects resolution output yet is excluded from the artifact cache key.
pub(in crate::analysis) const MAX_NAMES: usize = 1 << 20;

const _: () = assert!(MAX_NAMES == glass_lint_datastructures::DEFAULT_MAX_NAMES);
