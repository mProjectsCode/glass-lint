//! Stable identities for bindings, functions, objects, and canonical paths.
//!
//! Type definitions have been moved to [`crate::analysis::model::scope`] and
//! [`crate::analysis::model::value`]. This module retains only the free
//! identity-comparison functions.

use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};

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

    let expected_view = expected.as_view();
    let found_view = found.as_view();

    let Some(expected_root) = expected_view
        .first_segment()
        .copied()
        .and_then(|id| table.resolve(id))
    else {
        return false;
    };
    let Some(found_root) = found_view
        .first_segment()
        .copied()
        .and_then(|id| table.resolve(id))
    else {
        return false;
    };
    if environment.global_object_aliases_match(expected_root, found_root)
        && expected_view.tail_after(1) == found_view.tail_after(1)
    {
        return true;
    }

    if expected_view.len() > 1
        && expected_view
            .tail_after(1)
            .and_then(|tail| tail.first_segment())
            .copied()
            .and_then(|id| table.resolve(id))
            .is_some_and(|member| environment.is_promoted_global_member(expected_root, member))
        && expected_view.tail_after(1) == Some(found_view)
    {
        return true;
    }

    if found_view.len() > 1
        && found_view
            .tail_after(1)
            .and_then(|tail| tail.first_segment())
            .copied()
            .and_then(|id| table.resolve(id))
            .is_some_and(|member| environment.is_promoted_global_member(found_root, member))
        && found_view.tail_after(1) == Some(expected_view)
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
    environment.global_object_paths_match(expected, found)
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{NamePath, NameTable, SymbolPath};

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
