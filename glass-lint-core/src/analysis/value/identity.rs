//! Stable identities for bindings, functions, objects, and canonical paths.
//!
//! These types are intentionally opaque and orderable. Their equality is the
//! semantic identity used by flow/index maps; formatting is provided only for
//! human-readable symbol paths.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Canonical ID for an abstract value in one analysis arena.
pub(in crate::analysis) struct ValueId(pub(in crate::analysis) u32);

impl ValueId {
    /// Sentinel used whenever analysis cannot prove a value identity.
    pub const UNKNOWN: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identity for a lexical binding declaration.
pub(in crate::analysis) struct BindingId(pub(in crate::analysis) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Monotonic version of a binding after a source-order assignment.
pub(in crate::analysis) struct BindingVersion(pub(in crate::analysis) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable identity of a function used by helper-flow summaries.
pub(in crate::analysis) struct FunctionId(pub(in crate::analysis) u32);

/// Canonical member path represented as individual segments rather than a
/// formatted string, so identity and display concerns stay separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct SymbolPath(Vec<String>);

impl SymbolPath {
    /// Parse a dotted chain into canonical non-empty path segments.
    pub(in crate::analysis) fn from_chain(chain: &str) -> Self {
        Self(
            chain
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    /// Whether this path has at most one segment.
    pub(in crate::analysis) fn is_root(&self) -> bool {
        self.0.len() <= 1
    }

    /// Remove a terminal `.bind` segment when present.
    pub(in crate::analysis) fn without_bind_suffix(&self) -> Option<Self> {
        self.0
            .last()
            .is_some_and(|segment| segment == "bind")
            .then(|| Self(self.0[..self.0.len().saturating_sub(1)].to_vec()))
    }

    /// Append a dotted suffix without retaining an extra separator segment.
    pub(in crate::analysis) fn append_chain(&self, suffix: &str) -> Self {
        let mut path = self.0.clone();
        path.extend(
            suffix
                .strip_prefix('.')
                .unwrap_or(suffix)
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string),
        );
        Self(path)
    }
}

impl From<String> for SymbolPath {
    fn from(value: String) -> Self {
        Self::from_chain(&value)
    }
}
impl From<&str> for SymbolPath {
    fn from(value: &str) -> Self {
        Self::from_chain(value)
    }
}
impl fmt::Display for SymbolPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("."))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Root identity plus versioned member path for alias/flow keys.
pub(in crate::analysis) enum BindingRoot {
    /// A lexical binding qualified by enclosing function and assignment
    /// version.
    Binding {
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    },
    /// A configured/global root name.
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Canonical binding root with zero or more member segments.
pub(in crate::analysis) struct BindingKey {
    /// Stable root identity.
    root: BindingRoot,
    /// Static member path from the root.
    path: Vec<String>,
}

impl BindingKey {
    /// Create a key with no member segments.
    pub(in crate::analysis) fn new(root: BindingRoot) -> Self {
        Self {
            root,
            path: Vec::new(),
        }
    }

    /// Extend the key with one static member segment.
    pub(in crate::analysis) fn append_segment(&mut self, segment: String) {
        self.path.push(segment);
    }
}
