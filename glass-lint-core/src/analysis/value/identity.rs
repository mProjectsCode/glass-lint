//! Stable identities for bindings, functions, objects, and canonical paths.
//!
//! These types are intentionally opaque and orderable. Their equality is the
//! semantic identity used by flow/index maps; formatting is provided only for
//! human-readable symbol paths.

use std::fmt;

use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::analysis::name::{NameId, NameTable};

// ---------------------------------------------------------------------------
// Shared path-algebra helpers for NamePath and SymbolPath.
// Both types delegate slice-manipulation operations here so the logic has
// one implementation.
// ---------------------------------------------------------------------------

/// Return a new vector containing all but the last element.
fn path_without_last<T: Clone>(slice: &[T]) -> Option<Vec<T>> {
    if slice.is_empty() {
        None
    } else {
        Some(slice[..slice.len() - 1].to_vec())
    }
}

/// Return a new vector containing all but the first element.
fn path_without_first<T: Clone>(slice: &[T]) -> Option<Vec<T>> {
    if slice.is_empty() {
        None
    } else {
        Some(slice[1..].to_vec())
    }
}

/// Whether the path has at most one segment.
fn path_is_root<T>(slice: &[T]) -> bool {
    slice.len() <= 1
}

/// Whether `slice` starts with `prefix` at the segment level.
fn path_is_equal_or_descendant_of<T: PartialEq>(slice: &[T], root: &[T]) -> bool {
    slice.len() >= root.len() && slice[..root.len()] == root[..]
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Artifact-local member path. Unlike [`SymbolPath`], this path cannot be
/// compared across artifacts and therefore stores only canonical name IDs.
pub(in crate::analysis) struct NamePath(SmallVec<[NameId; 4]>);

#[allow(dead_code)]
impl NamePath {
    pub(in crate::analysis) fn new() -> Self {
        Self(SmallVec::new())
    }

    pub(in crate::analysis) fn append(&mut self, segment: NameId) {
        self.0.push(segment);
    }

    pub(in crate::analysis) fn segments(&self) -> &[NameId] {
        &self.0
    }

    pub(in crate::analysis) fn first_segment(&self) -> Option<NameId> {
        self.0.first().copied()
    }

    pub(in crate::analysis) fn last_segment(&self) -> Option<NameId> {
        self.0.last().copied()
    }

    pub(in crate::analysis) fn without_last_segment(&self) -> Option<Self> {
        path_without_last(&self.0).map(|v| Self(SmallVec::from_vec(v)))
    }

    pub(in crate::analysis) fn without_first_segment(&self) -> Option<Self> {
        path_without_first(&self.0).map(|v| Self(SmallVec::from_vec(v)))
    }

    pub(in crate::analysis) fn append_path(&self, suffix: &Self) -> Self {
        let mut path = self.clone();
        path.0.extend(suffix.0.iter().copied());
        path
    }

    pub(in crate::analysis) fn is_root(&self) -> bool {
        path_is_root(&self.0)
    }

    pub(in crate::analysis) fn without_segment(&self, name: &str, table: &NameTable) -> Self {
        if self.first_segment().and_then(|id| table.resolve(id)) == Some(name) {
            return self.without_first_segment().unwrap_or_default();
        }
        self.clone()
    }

    pub(in crate::analysis) fn without_this_prefix(&self, table: &NameTable) -> Self {
        self.without_segment("this", table)
    }

    pub(in crate::analysis) fn without_bind_suffix(&self, table: &NameTable) -> Option<Self> {
        (self.last_segment().and_then(|id| table.resolve(id)) == Some("bind"))
            .then(|| self.without_last_segment())
            .flatten()
    }

    pub(in crate::analysis) fn from_symbol_path(
        path: &SymbolPath,
        table: &NameTable,
    ) -> Option<Self> {
        path.segments()
            .iter()
            .map(|segment| table.lookup(segment))
            .collect::<Option<SmallVec<[NameId; 4]>>>()
            .map(Self)
    }

    pub(in crate::analysis) fn is_equal_or_descendant_of(&self, root: &Self) -> bool {
        path_is_equal_or_descendant_of(&self.0, &root.0)
    }

    pub(in crate::analysis) fn to_symbol_path(&self, table: &NameTable) -> Option<SymbolPath> {
        self.0
            .iter()
            .map(|id| table.resolve(*id).map(SmolStr::new))
            .collect::<Option<Vec<_>>>()
            .map(SymbolPath::from_segments)
    }

    /// Compare an artifact-local path with another artifact-local path using
    /// the environment's rooted-object alias policy. Only root/member names
    /// are resolved for the policy checks; ordinary path segments stay IDs.
    pub(crate) fn matches_global_object_alias_with(
        &self,
        found: &Self,
        table: &NameTable,
        environment: &crate::Environment,
    ) -> bool {
        if self == found {
            return true;
        }

        let Some(expected_root) = self.first_segment().and_then(|id| table.resolve(id)) else {
            return false;
        };
        let Some(found_root) = found.first_segment().and_then(|id| table.resolve(id)) else {
            return false;
        };
        if environment.global_object_aliases_match(expected_root, found_root)
            && self.segments().get(1..) == found.segments().get(1..)
        {
            return true;
        }

        if self.segments().len() > 1
            && environment
                .global_objects()
                .any(|object| object == expected_root)
            && self
                .segments()
                .get(1)
                .and_then(|id| table.resolve(*id))
                .is_some_and(|member| environment.is_global_member(expected_root, member))
            && self.segments().get(1..) == Some(found.segments())
        {
            return true;
        }

        if found.segments().len() > 1
            && environment
                .global_objects()
                .any(|object| object == found_root)
            && found
                .segments()
                .get(1)
                .and_then(|id| table.resolve(*id))
                .is_some_and(|member| environment.is_global_member(found_root, member))
            && found.segments().get(1..) == Some(self.segments())
        {
            return true;
        }

        false
    }

    pub(in crate::analysis) fn from_ids(ids: impl IntoIterator<Item = NameId>) -> Self {
        Self(ids.into_iter().collect())
    }
}

/// Canonical member path represented as individual segments rather than a
/// formatted string, so identity and display concerns stay separate.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolPath(Vec<SmolStr>);

impl SymbolPath {
    pub(in crate::analysis) fn from_segments(segments: Vec<SmolStr>) -> Self {
        Self(segments)
    }

    pub(in crate::analysis) fn first_segment(&self) -> Option<&str> {
        self.0.first().map(SmolStr::as_str)
    }

    pub(in crate::analysis) fn without_last_segment(&self) -> Option<Self> {
        path_without_last(&self.0).map(Self)
    }

    pub(in crate::analysis) fn without_first_segment(&self) -> Option<Self> {
        path_without_first(&self.0).map(Self)
    }

    pub(in crate::analysis) fn segments(&self) -> &[SmolStr] {
        &self.0
    }

    pub(in crate::analysis) fn append_path(&self, suffix: &Self) -> Self {
        let mut path = self.0.clone();
        path.extend(suffix.0.iter().cloned());
        Self(path)
    }

    pub(crate) fn eq_chain(&self, chain: &str) -> bool {
        self.0.iter().map(SmolStr::as_str).eq(chain.split('.'))
    }

    /// Parse a dotted chain into canonical non-empty path segments.
    pub fn from_chain(chain: &str) -> Self {
        Self(
            chain
                .split('.')
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(SmolStr::new)
                .collect(),
        )
    }

    /// Return true when the path has zero segments.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether this path has at most one segment.
    pub(in crate::analysis) fn is_root(&self) -> bool {
        path_is_root(&self.0)
    }

    /// Remove a terminal `.bind` segment when present.
    pub(in crate::analysis) fn without_bind_suffix(&self) -> Option<Self> {
        self.0
            .last()
            .is_some_and(|segment| segment == "bind")
            .then(|| self.without_last_segment())
            .flatten()
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
                .map(SmolStr::new),
        );
        Self(path)
    }

    /// Compare rooted paths using the configured environment's realm-object
    /// policy. No host spelling is implicitly recognized here.
    pub(crate) fn matches_global_object_alias(
        &self,
        found: &Self,
        environment: &crate::Environment,
    ) -> bool {
        environment.global_object_paths_match(&self.0, &found.0)
    }

    /// Remove the syntax-only `this.` prefix from a rooted path.
    pub(in crate::analysis) fn without_this_prefix(&self) -> Self {
        if self.0.first().is_some_and(|segment| segment == "this") {
            Self(self.0[1..].to_vec())
        } else {
            self.clone()
        }
    }

    /// Return whether this path is equal to or below another path.
    pub(crate) fn is_equal_or_descendant_of(&self, root: &Self) -> bool {
        path_is_equal_or_descendant_of(&self.0, &root.0)
    }
}

impl From<String> for SymbolPath {
    fn from(value: String) -> Self {
        Self::from_chain(&value)
    }
}
impl From<SmolStr> for SymbolPath {
    fn from(value: SmolStr) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_paths_use_the_same_table_for_checked_conversion() {
        let mut names = NameTable::default();
        let client = names.intern("client").unwrap();
        let request = names.intern("request").unwrap();
        let path = NamePath::from_ids([client, request]);

        assert_eq!(path.first_segment(), Some(client));
        assert_eq!(path.last_segment(), Some(request));
        assert_eq!(
            path.to_symbol_path(&names),
            Some(SymbolPath::from("client.request"))
        );
        assert_eq!(
            NamePath::from_symbol_path(&SymbolPath::from("client.request"), &names),
            Some(path)
        );
    }

    #[test]
    fn missing_compiled_segments_fail_closed() {
        let mut names = NameTable::default();
        names.intern("client").unwrap();

        assert!(NamePath::from_symbol_path(&SymbolPath::from("client.request"), &names).is_none());
    }

    #[test]
    fn id_only_path_operations_preserve_segment_identity() {
        let mut names = NameTable::default();
        let this = names.intern("this").unwrap();
        let client = names.intern("client").unwrap();
        let bind = names.intern("bind").unwrap();
        let send = names.intern("send").unwrap();
        let path = NamePath::from_ids([this, client, bind]);

        assert_eq!(
            path.without_this_prefix(&names),
            NamePath::from_ids([client, bind])
        );
        assert_eq!(
            path.without_bind_suffix(&names),
            Some(NamePath::from_ids([this, client]))
        );
        assert_eq!(
            path.append_path(&NamePath::from_ids([send])),
            NamePath::from_ids([this, client, bind, send])
        );
        assert!(!path.is_root());
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
    /// Artifact-local static member path from the root.
    path: NamePath,
}

impl BindingKey {
    /// Create a key with no member segments.
    pub(in crate::analysis) fn new(root: BindingRoot) -> Self {
        Self {
            root,
            path: NamePath::new(),
        }
    }

    /// Extend the key with one static member segment.
    pub(in crate::analysis) fn append_segment(&mut self, segment: NameId) {
        self.path.append(segment);
    }
}
