//! Value identities and bounded interning.
//!
//! The value layer gives semantic analysis canonical, hashable identities for
//! bindings, callables, objects, and paths. Every arena and interner is
//! bounded; exhaustion maps to an explicit unknown result (`ValueId::UNKNOWN`)
//! rather than an invented ID or a panic.
//!
//! Global-object identity comparison remains here because it coordinates the
//! environment policy with artifact-local name paths. Retained value types and
//! bounded tables live in `analysis::model`.
//!
//! Path trie types (`PathId`, `PathSegment`, `PathStore`) live in
//! [`glass_lint_datastructures`] and are imported
//! directly by callers.

mod identity;

pub use identity::{matches_global_object_alias, matches_global_object_alias_with};
