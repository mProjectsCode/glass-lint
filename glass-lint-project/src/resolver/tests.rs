use glass_lint_core::project::{ProjectRelativePath, ResolutionRequestKey};
use glass_lint_datastructures::{Position, SourceRange};

use super::*;
use crate::options::ProjectLoadOptions;

fn request(specifier: &str) -> ResolutionRequest {
    ResolutionRequest::new(
        ResolutionRequestKey::new(
            ProjectRelativePath::new("main.js").unwrap(),
            ResolutionRequestKind::StaticImport,
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap()).unwrap(),
        ),
        specifier,
    )
}

fn require_request(specifier: &str) -> ResolutionRequest {
    ResolutionRequest::new(
        ResolutionRequestKey::new(
            ProjectRelativePath::new("main.js").unwrap(),
            ResolutionRequestKind::Require,
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap()).unwrap(),
        ),
        specifier,
    )
}

fn with_resolver(f: impl FnOnce(&ProjectResolver)) {
    let options = ProjectLoadOptions::default().validated().unwrap();
    let resolver = ProjectResolver::new(
        SourceBoundary::new(Path::new("."), &options).unwrap(),
        &ProjectSelection::entry("main.js"),
    )
    .unwrap();
    f(&resolver);
}

#[test]
fn delegates_builtin_detection_and_canonicalization_to_oxc() {
    with_resolver(|resolver| {
        for (specifier, expected) in [
            ("fs", "node:fs"),
            ("node:fs", "node:fs"),
            ("assert/strict", "node:assert/strict"),
            ("timers/promises", "node:timers/promises"),
        ] {
            assert_eq!(
                resolver.resolve(&request(specifier)).unwrap(),
                ResolverOutcome::Builtin {
                    name: BuiltinModuleName::new(expected).unwrap(),
                },
                "specifier: {specifier}"
            );
        }
    });
}

#[test]
fn unresolved_bare_packages_remain_external() {
    with_resolver(|resolver| {
        assert_eq!(
            resolver.resolve(&request("not-a-node-builtin")).unwrap(),
            ResolverOutcome::External {
                package: PackageSpecifier::new("not-a-node-builtin").unwrap(),
            }
        );
    });
}

#[test]
fn require_and_import_resolve_builtins_identically() {
    with_resolver(|resolver| {
        let import_result = resolver.resolve(&request("node:fs")).unwrap();
        let require_result = resolver.resolve(&require_request("node:fs")).unwrap();
        assert_eq!(import_result, require_result);
    });
}

#[test]
fn package_name_extracts_scoped_and_non_scoped() {
    assert_eq!(package_name("lodash"), "lodash");
    assert_eq!(package_name("@scope/pkg"), "@scope/pkg");
    assert_eq!(package_name("@scope/pkg/helpers"), "@scope/pkg");
    assert_eq!(package_name("lodash/merge"), "lodash");
}

#[test]
fn package_name_falls_back_on_empty() {
    assert_eq!(package_name(""), "");
}

#[test]
fn miss_returns_missing_for_internal_looking_requests() {
    with_resolver(|resolver| {
        let result = resolver.resolve(&request("./nonexistent")).unwrap();
        assert_eq!(result, ResolverOutcome::Missing);
    });
}

#[test]
fn malformed_scoped_package_returns_unsupported_not_external() {
    with_resolver(|resolver| {
        for specifier in ["@", "@/", "@scope"] {
            let result = resolver.resolve(&request(specifier)).unwrap();
            assert!(
                matches!(result, ResolverOutcome::Unsupported { .. }),
                "specifier `{specifier}` should be Unsupported, got {result:?}"
            );
        }
    });
}

#[test]
fn empty_specifier_returns_unsupported() {
    with_resolver(|resolver| {
        let result = resolver.resolve(&request("")).unwrap();
        assert!(
            matches!(result, ResolverOutcome::Unsupported { .. }),
            "empty specifier should be Unsupported, got {result:?}"
        );
    });
}

#[test]
fn ordinary_absent_bare_package_stays_external() {
    with_resolver(|resolver| {
        for specifier in ["nonexistent-pkg", "@scope/pkg"] {
            let result = resolver.resolve(&request(specifier)).unwrap();
            assert!(
                matches!(result, ResolverOutcome::External { .. }),
                "specifier `{specifier}` should be External, got {result:?}"
            );
        }
    });
}
