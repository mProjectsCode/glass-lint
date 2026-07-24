//! Dense tables for identities allocated by the fact builder.
//!
//! The generic [`IndexTable`] is owned by `glass-lint-datastructures`;
//! this module provides a function-keyed specialization.

use crate::analysis::value::FunctionId;

/// Sparse dense-indexed storage for function identities.
///
/// Missing slots are valid and represent functions that were not emitted or
/// exceeded the enclosing analysis budget; callers must handle `None`.
pub(in crate::analysis) type FunctionTable<T> =
    glass_lint_datastructures::IndexTable<FunctionId, T>;
