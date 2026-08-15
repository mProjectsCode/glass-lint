use super::*;

#[test]
fn package_patterns_obey_boundaries() {
    let pattern = ModuleSpecifierPattern::package("@scope/pkg").unwrap();
    assert!(pattern.matches("@scope/pkg"));
    assert!(pattern.matches("@scope/pkg/subpath"));
    assert!(!pattern.matches("@scope/pkg-extra"));
    assert!(!pattern.matches("@scope/pkgx/subpath"));
}

#[test]
fn package_patterns_reject_non_packages() {
    for value in [
        "",
        "pkg/",
        "pkg/subpath",
        "./pkg",
        "/pkg",
        "https://pkg",
        "pkg foo",
        "\\foo",
        "pkg\0foo",
    ] {
        assert!(ModuleSpecifierPattern::package(value).is_err(), "{value}");
    }
}

#[test]
fn display_impl_shows_name() {
    let pkg = ModuleSpecifierPattern::package("bar").unwrap();
    assert_eq!(format!("{pkg}"), "bar");
}

#[test]
fn scoped_package_rejects_empty_scope_or_name() {
    let result = ModuleSpecifierPattern::package("@/pkg");
    assert!(result.is_err());
}
