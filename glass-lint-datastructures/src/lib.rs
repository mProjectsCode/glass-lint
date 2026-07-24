//! Provider-neutral data structures for analysis.
//!
//! This crate provides the core data types used across Glass Lint's analysis
//! engine: interned names, sparse tables, bounded budgets, path
//! representations, fingerprint hashing, and source-location diagnostics.
//! Every structure enforces its invariants at construction time and is designed
//! for deterministic, bounded, allocation-conscious analysis.

pub mod budget;
pub mod diagnostic;
pub mod fingerprint;
pub mod name;
pub mod path;
pub mod path_trie;
pub mod table;

pub use budget::{Budget, BudgetTracker};
pub use diagnostic::{
    ByteRange, InvalidPosition, InvalidSourceBoundary, Position, ReversedByteRange,
    ReversedSourcePositionRange, SourceRange,
};
pub use fingerprint::{Fingerprint, fnv_init, fnv_write};
pub use name::{NameExhausted, NameId, NameTable};
pub use path::{NamePath, Path, PathView, SymbolPath};
pub use path_trie::{ParentPathStore, PathId, PathInterner, PathNode, PathSegment, PathSegments};
pub use table::{IdIndex, IndexTable};
