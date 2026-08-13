//! Oxc-backed module resolution and provider-neutral result classification.

use std::path::Path;

use glass_lint_core::project::{
    BuiltinModuleName, NormalizedOutsidePath, PackageSpecifier, ResolutionRequest,
    ResolutionRequestKind, ResolverOutcome, is_internal_module_request,
};
use oxc_resolver::{ResolveError, ResolveOptions, Resolver};

use crate::{
    boundary::{PathClassification, SourceBoundary, absolute_path},
    error::ProjectLoadError,
    options::ProjectSelection,
};

/// Keeps import and CommonJS resolution policy together for one project.
pub struct ProjectResolver<'a> {
    boundary: SourceBoundary<'a>,
    import: Resolver,
    require: Resolver,
}

impl<'a> ProjectResolver<'a> {
    /// Build import and CommonJS resolvers under one project root.
    pub fn new(
        boundary: SourceBoundary<'a>,
        selection: &ProjectSelection,
    ) -> Result<Self, ProjectLoadError> {
        let options = boundary.options();
        let extension_alias = options
            .extension_aliases()
            .map(|(key, value)| (key.to_owned(), value.to_vec()))
            .collect();
        let mut base = ResolveOptions {
            extensions: options.extensions().map(str::to_owned).collect(),
            extension_alias,
            symlinks: options.follow_symlinks(),
            roots: vec![boundary.canonical_root().to_path_buf()],
            builtin_modules: true,
            ..ResolveOptions::default()
        };
        if let ProjectSelection::Tsconfig(path) = selection {
            base.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Manual(
                oxc_resolver::TsconfigOptions {
                    config_file: absolute_path(path)?,
                    references: oxc_resolver::TsconfigReferences::Auto,
                },
            ));
        }
        let import = Resolver::new(ResolveOptions {
            condition_names: vec!["node".into(), "import".into()],
            ..base.clone()
        });
        let require = import.clone_with_options(ResolveOptions {
            condition_names: vec!["node".into(), "require".into()],
            ..base
        });
        Ok(Self {
            boundary,
            import,
            require,
        })
    }

    /// Resolve one request into a provider-neutral, root-classified outcome.
    pub fn resolve(
        &self,
        request: &ResolutionRequest,
    ) -> Result<ResolverOutcome, ProjectLoadError> {
        let importer = self.boundary.canonical_root().join(request.importer());
        let directory = importer
            .parent()
            .unwrap_or_else(|| self.boundary.canonical_root());
        let resolver = if request.kind() == ResolutionRequestKind::Require {
            &self.require
        } else {
            &self.import
        };
        match resolver.resolve(directory, request.specifier()) {
            Ok(resolution) => self.classify(request.specifier(), resolution.path()),
            Err(ResolveError::Builtin { resolved, .. }) => Ok(ResolverOutcome::Builtin {
                name: BuiltinModuleName::new(resolved)?,
            }),
            // Deliberate not-found: bare packages remain external, internal
            // requests become missing.
            Err(ResolveError::NotFound(_) | ResolveError::MatchedAliasNotFound(..))
                if is_internal_module_request(request.specifier()) =>
            {
                Ok(ResolverOutcome::Missing)
            }
            Err(ResolveError::NotFound(_) | ResolveError::MatchedAliasNotFound(..)) => Ok(
                external_outcome(request.specifier(), " in not-found request"),
            ),
            // All other resolver errors (I/O, specifier, config, etc.) are
            // operational or invalid — fail closed as unsupported.
            Err(other) => Ok(ResolverOutcome::Unsupported {
                reason: format!("resolution failed: {other}"),
            }),
        }
    }

    fn classify(&self, request: &str, path: &Path) -> Result<ResolverOutcome, ProjectLoadError> {
        let classification = self.boundary.classify(path)?;
        let internal = is_internal_module_request(request);
        Ok(match classification {
            PathClassification::Outside(path) => {
                if internal {
                    ResolverOutcome::OutsideProject {
                        path: NormalizedOutsidePath::new(
                            path.as_ref().to_string_lossy().into_owned(),
                        )?,
                    }
                } else {
                    external_outcome(request, "")
                }
            }
            PathClassification::Excluded(path) => {
                if internal {
                    ResolverOutcome::Unsupported {
                        reason: format!("excluded target `{}`", path.as_ref().display()),
                    }
                } else {
                    external_outcome(request, "")
                }
            }
            PathClassification::Unsupported(path) => ResolverOutcome::Unsupported {
                reason: format!("unsupported target `{}`", path.as_ref().display()),
            },
            PathClassification::Accepted(accepted) => ResolverOutcome::Internal {
                path: accepted.relative().clone(),
            },
        })
    }
}

fn external_outcome(request: &str, context: &str) -> ResolverOutcome {
    match PackageSpecifier::new(package_name(request)) {
        Ok(package) => ResolverOutcome::External { package },
        Err(error) => ResolverOutcome::Unsupported {
            reason: format!("invalid package specifier{context}: {error}"),
        },
    }
}

fn package_name(request: &str) -> String {
    if request.starts_with('@') {
        request.split('/').take(2).collect::<Vec<_>>().join("/")
    } else {
        request.split('/').next().unwrap_or(request).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_core::project::{ProjectRelativePath, ResolutionRequestKey};
    use glass_lint_datastructures::{Position, SourceRange};

    use super::*;
    use crate::options::ProjectLoadOptions;

    fn request(specifier: &str) -> ResolutionRequest {
        ResolutionRequest::new(
            ResolutionRequestKey::new(
                ProjectRelativePath::new("main.js").unwrap(),
                ResolutionRequestKind::StaticImport,
                SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap())
                    .unwrap(),
            ),
            specifier,
        )
    }

    fn require_request(specifier: &str) -> ResolutionRequest {
        ResolutionRequest::new(
            ResolutionRequestKey::new(
                ProjectRelativePath::new("main.js").unwrap(),
                ResolutionRequestKind::Require,
                SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap())
                    .unwrap(),
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
}
