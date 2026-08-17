use super::*;

#[test]
fn source_file_reuses_arc_source_allocation() {
    let source: Arc<str> = Arc::from("fetch('/remote');");
    let file = SourceFile::new("main.js", source.clone()).unwrap();

    assert!(std::ptr::eq(source.as_ref(), file.source().as_str()));
}

// ── PackageSpecifier ──────────────────────────────────────────

#[test]
fn package_specifier_rejects_empty() {
    assert!(PackageSpecifier::new("").is_err());
}

#[test]
fn package_specifier_rejects_whitespace_only() {
    assert!(PackageSpecifier::new("  ").is_err());
    assert!(PackageSpecifier::new("\t").is_err());
}

#[test]
fn package_specifier_strips_surrounding_whitespace() {
    let pkg = PackageSpecifier::new("  lodash  ").unwrap();
    assert_eq!(pkg.as_str(), "lodash");
}

#[test]
fn package_specifier_rejects_interior_whitespace() {
    assert!(PackageSpecifier::new("lodash foo").is_err());
    assert!(PackageSpecifier::new("lodash\tfoo").is_err());
}

#[test]
fn package_specifier_rejects_nul() {
    assert!(PackageSpecifier::new("lod\0ash").is_err());
}

#[test]
fn package_specifier_rejects_relative_syntax() {
    assert!(PackageSpecifier::new("./foo").is_err());
    assert!(PackageSpecifier::new("../foo").is_err());
    assert!(PackageSpecifier::new("/foo").is_err());
    assert!(PackageSpecifier::new("\\foo").is_err());
}

#[test]
fn package_specifier_rejects_non_scoped_with_slash() {
    assert!(PackageSpecifier::new("lodash/fp").is_err());
    assert!(PackageSpecifier::new("a/b/c").is_err());
}

#[test]
fn package_specifier_rejects_bare_at() {
    assert!(PackageSpecifier::new("@").is_err());
}

#[test]
fn package_specifier_rejects_scoped_missing_name() {
    assert!(PackageSpecifier::new("@scope/").is_err());
}

#[test]
fn package_specifier_rejects_scoped_missing_scope() {
    assert!(PackageSpecifier::new("@/name").is_err());
}

#[test]
fn package_specifier_rejects_scoped_double_slash() {
    assert!(PackageSpecifier::new("@scope//name").is_err());
}

#[test]
fn package_specifier_accepts_valid_scoped() {
    let pkg = PackageSpecifier::new("@angular/core").unwrap();
    assert_eq!(pkg.as_str(), "@angular/core");
}

#[test]
fn package_specifier_accepts_valid_bare() {
    let pkg = PackageSpecifier::new("lodash").unwrap();
    assert_eq!(pkg.as_str(), "lodash");
    let pkg = PackageSpecifier::new("express").unwrap();
    assert_eq!(pkg.as_str(), "express");
}

#[test]
fn package_specifier_equality_with_str() {
    let pkg = PackageSpecifier::new("lodash").unwrap();
    assert_eq!(pkg, "lodash");
}

// ── BuiltinModuleName ─────────────────────────────────────────

#[test]
fn builtin_rejects_empty() {
    assert!(BuiltinModuleName::new("").is_err());
}

#[test]
fn builtin_rejects_whitespace_only() {
    assert!(BuiltinModuleName::new("  ").is_err());
}

#[test]
fn builtin_strips_surrounding_whitespace() {
    let name = BuiltinModuleName::new("  node:fs  ").unwrap();
    assert_eq!(name.as_str(), "node:fs");
}

#[test]
fn builtin_rejects_interior_whitespace() {
    assert!(BuiltinModuleName::new("node: fs").is_err());
    assert!(BuiltinModuleName::new("node:f s").is_err());
    assert!(BuiltinModuleName::new("no de:fs").is_err());
    assert!(BuiltinModuleName::new("node :fs").is_err());
}

#[test]
fn builtin_rejects_nul() {
    assert!(BuiltinModuleName::new("node:f\0s").is_err());
}

#[test]
fn builtin_rejects_missing_scheme_or_name() {
    assert!(BuiltinModuleName::new("fs").is_err());
    assert!(BuiltinModuleName::new("nodefs").is_err());
    assert!(BuiltinModuleName::new(":fs").is_err());
}

#[test]
fn builtin_rejects_empty_name() {
    assert!(BuiltinModuleName::new("node:").is_err());
}

#[test]
fn builtin_accepts_valid_names() {
    let name = BuiltinModuleName::new("node:fs").unwrap();
    assert_eq!(name.as_str(), "node:fs");
    let name = BuiltinModuleName::new("node:path").unwrap();
    assert_eq!(name.as_str(), "node:path");
    let name = BuiltinModuleName::new("node:buffer").unwrap();
    assert_eq!(name.as_str(), "node:buffer");
    let name = BuiltinModuleName::new("deno:fs").unwrap();
    assert_eq!(name.as_str(), "deno:fs");
    let name = BuiltinModuleName::new("Node:fs").unwrap();
    assert_eq!(name.as_str(), "Node:fs");
}

#[test]
fn builtin_equality_with_str() {
    let name = BuiltinModuleName::new("node:fs").unwrap();
    assert_eq!(name, "node:fs");
}

// ── NormalizedOutsidePath ─────────────────────────────────────

#[test]
fn outside_path_rejects_empty() {
    assert!(NormalizedOutsidePath::new("").is_err());
}

#[test]
fn outside_path_rejects_nul() {
    assert!(NormalizedOutsidePath::new("foo\0bar").is_err());
}

#[test]
fn outside_path_normalizes_backslashes() {
    let path = NormalizedOutsidePath::new("a\\b\\c").unwrap();
    assert_eq!(path.as_str(), "a/b/c");
}

#[test]
fn outside_path_normalizes_dot_segments() {
    let path = NormalizedOutsidePath::new("a/./b").unwrap();
    assert_eq!(path.as_str(), "a/b");
}

#[test]
fn outside_path_normalizes_relative_parent() {
    let path = NormalizedOutsidePath::new("a/b/../c").unwrap();
    assert_eq!(path.as_str(), "a/c");
}

#[test]
fn outside_path_preserves_absolute() {
    let path = NormalizedOutsidePath::new("/a/b").unwrap();
    assert_eq!(path.as_str(), "/a/b");
}

#[test]
fn outside_path_identity_round_trip() {
    let path = NormalizedOutsidePath::new("/usr/lib/node_modules/foo").unwrap();
    assert_eq!(path.as_str(), "/usr/lib/node_modules/foo");
}
