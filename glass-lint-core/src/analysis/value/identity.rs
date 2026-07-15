//! Stable identities for bindings, functions, objects, and canonical paths.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct ValueId(pub(in crate::analysis) u32);

impl ValueId {
    /// Sentinel used whenever analysis cannot prove a value identity.
    pub const UNKNOWN: Self = Self(0);
}

/// Stable identity for a lexical binding declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct BindingId(pub(in crate::analysis) u32);

/// Monotonic version of a binding after a source-order assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct BindingVersion(pub(in crate::analysis) u32);

/// Stable identity of a function used by helper-flow summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct FunctionId(pub(in crate::analysis) u32);

/// Canonical member path represented as individual segments rather than a
/// formatted string, so identity and display concerns stay separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct SymbolPath(Vec<String>);

impl SymbolPath {
    pub(in crate::analysis) fn from_chain(chain: &str) -> Self {
        Self(
            chain
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }
    pub(in crate::analysis) fn is_root(&self) -> bool {
        self.0.len() <= 1
    }
    pub(in crate::analysis) fn without_bind_suffix(&self) -> Option<Self> {
        self.0
            .last()
            .is_some_and(|segment| segment == "bind")
            .then(|| Self(self.0[..self.0.len().saturating_sub(1)].to_vec()))
    }
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
pub(in crate::analysis) enum BindingRoot {
    Binding {
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    },
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct BindingKey {
    root: BindingRoot,
    path: Vec<String>,
}

impl BindingKey {
    pub(in crate::analysis) fn new(root: BindingRoot) -> Self {
        Self {
            root,
            path: Vec::new(),
        }
    }

    pub(in crate::analysis) fn append_segment(&mut self, segment: String) {
        self.path.push(segment);
    }
}
