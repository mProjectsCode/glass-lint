//! Stable identities for bindings, functions, objects, and canonical paths.
//!
//! These types are intentionally opaque and orderable. Their equality is the
//! semantic identity used by flow/index maps; formatting is provided only for
//! human-readable symbol paths.
//!
//! `NamePath` and `SymbolPath` live in [`glass_lint_datastructures::path`];
//! this module provides NameTable-dependent free functions that operate on
//! them.

use glass_lint_datastructures::{NameId, NamePath, NameTable, SymbolPath};

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

/// Compare two [`NamePath`]s using the environment's rooted-object alias
/// policy.
pub fn matches_global_object_alias_with(
    found: &NamePath,
    expected: &NamePath,
    table: &NameTable,
    environment: &crate::Environment,
) -> bool {
    if found == expected {
        return true;
    }

    let Some(expected_root) = expected
        .first_segment()
        .copied()
        .and_then(|id| table.resolve(id))
    else {
        return false;
    };
    let Some(found_root) = found
        .first_segment()
        .copied()
        .and_then(|id| table.resolve(id))
    else {
        return false;
    };
    if environment.global_object_aliases_match(expected_root, found_root)
        && expected.segments().get(1..) == found.segments().get(1..)
    {
        return true;
    }

    if expected.segments().len() > 1
        && environment
            .global_objects()
            .any(|object| object == expected_root)
        && expected
            .segments()
            .get(1)
            .copied()
            .and_then(|id| table.resolve(id))
            .is_some_and(|member| environment.is_global_member(expected_root, member))
        && expected.segments().get(1..) == Some(found.segments())
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
            .copied()
            .and_then(|id| table.resolve(id))
            .is_some_and(|member| environment.is_global_member(found_root, member))
        && found.segments().get(1..) == Some(expected.segments())
    {
        return true;
    }

    false
}

/// Compare two [`SymbolPath`]s using the environment's realm-object policy.
pub fn matches_global_object_alias(
    expected: &SymbolPath,
    found: &SymbolPath,
    environment: &crate::Environment,
) -> bool {
    environment.global_object_paths_match(expected.segments(), found.segments())
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

        assert_eq!(path.first_segment(), Some(&client));
        assert_eq!(path.last_segment(), Some(&request));
        assert_eq!(
            names.resolve_path(&path),
            Some(SymbolPath::from("client.request"))
        );
        assert_eq!(
            names.lookup_path(&SymbolPath::from("client.request")),
            Some(path)
        );
    }

    #[test]
    fn missing_compiled_segments_fail_closed() {
        let mut names = NameTable::default();
        names.intern("client").unwrap();

        assert!(
            names
                .lookup_path(&SymbolPath::from("client.request"))
                .is_none()
        );
    }

    #[test]
    fn id_only_path_operations_preserve_segment_identity() {
        fn without_segment(path: &NamePath, name: &str, table: &NameTable) -> NamePath {
            if path
                .first_segment()
                .copied()
                .and_then(|id| table.resolve(id))
                == Some(name)
            {
                path.without_first_segment().unwrap_or_default()
            } else {
                path.clone()
            }
        }
        fn without_this_prefix(path: &NamePath, table: &NameTable) -> NamePath {
            without_segment(path, "this", table)
        }
        fn without_bind_suffix(path: &NamePath, table: &NameTable) -> Option<NamePath> {
            (path
                .last_segment()
                .copied()
                .and_then(|id| table.resolve(id))
                == Some("bind"))
            .then(|| path.without_last_segment())
            .flatten()
        }

        let mut names = NameTable::default();
        let this = names.intern("this").unwrap();
        let client = names.intern("client").unwrap();
        let bind = names.intern("bind").unwrap();
        let send = names.intern("send").unwrap();
        let path = NamePath::from_ids([this, client, bind]);

        assert_eq!(
            without_this_prefix(&path, &names),
            NamePath::from_ids([client, bind])
        );
        assert_eq!(
            without_bind_suffix(&path, &names),
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
