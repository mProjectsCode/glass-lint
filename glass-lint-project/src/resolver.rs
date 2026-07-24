//! Oxc-backed module resolution and provider-neutral result classification.

use std::path::Path;

use glass_lint_core::project::{
    BuiltinModuleName, NormalizedOutsidePath, PackageSpecifier, ResolutionRequest,
    ResolutionRequestKind, ResolverOutcome, is_internal_module_request,
};
use oxc_resolver::{ResolveError, ResolveOptions, Resolver};

use crate::{
    admission::{PathAdmission, SourceAdmission, absolute_path},
    error::ProjectLoadError,
    options::ProjectSelection,
};

/// Keeps import and CommonJS resolution policy together for one project.
pub struct ProjectResolver<'a> {
    admission: SourceAdmission<'a>,
    import: Resolver,
    require: Resolver,
}

impl<'a> ProjectResolver<'a> {
    /// Build import and CommonJS resolvers under one project root.
    pub fn new(
        admission: SourceAdmission<'a>,
        selection: &ProjectSelection,
    ) -> Result<Self, ProjectLoadError> {
        let options = admission.options();
        let extension_alias = options
            .extension_aliases()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut base = ResolveOptions {
            extensions: options.extensions().map(str::to_owned).collect(),
            extension_alias,
            symlinks: options.follow_symlinks(),
            roots: vec![admission.canonical_root().to_path_buf()],
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
            admission,
            import,
            require,
        })
    }

    /// Resolve one request into a provider-neutral, root-classified outcome.
    pub fn resolve(
        &self,
        request: &ResolutionRequest,
    ) -> Result<ResolverOutcome, ProjectLoadError> {
        let importer = self.admission.canonical_root().join(&request.key.importer);
        let directory = importer
            .parent()
            .unwrap_or_else(|| self.admission.canonical_root());
        let resolver = if request.key.kind == ResolutionRequestKind::Require {
            &self.require
        } else {
            &self.import
        };
        match resolver.resolve(directory, &request.request) {
            Ok(resolution) => self.classify(&request.request, resolution.path()),
            Err(ResolveError::Builtin { resolved, .. }) => Ok(ResolverOutcome::Builtin {
                name: BuiltinModuleName::new(resolved)?,
            }),
            Err(_) if is_internal_module_request(&request.request) => Ok(ResolverOutcome::Missing),
            Err(_) => Ok(ResolverOutcome::External {
                package: PackageSpecifier::new(package_name(&request.request))?,
            }),
        }
    }

    fn classify(&self, request: &str, path: &Path) -> Result<ResolverOutcome, ProjectLoadError> {
        let admission = self.admission.classify(path)?;
        let internal = is_internal_module_request(request);
        Ok(match admission {
            PathAdmission::Outside(path) => {
                if internal {
                    ResolverOutcome::OutsideProject {
                        path: NormalizedOutsidePath::new(
                            path.as_ref().to_string_lossy().into_owned(),
                        )?,
                    }
                } else {
                    ResolverOutcome::External {
                        package: PackageSpecifier::new(package_name(request))?,
                    }
                }
            }
            PathAdmission::Excluded(path) => {
                if internal {
                    ResolverOutcome::Unsupported {
                        reason: format!("excluded target `{}`", path.as_ref().display()),
                    }
                } else {
                    ResolverOutcome::External {
                        package: PackageSpecifier::new(package_name(request))?,
                    }
                }
            }
            PathAdmission::Unsupported(path) => ResolverOutcome::Unsupported {
                reason: format!("unsupported target `{}`", path.as_ref().display()),
            },
            PathAdmission::Admitted(admitted) => ResolverOutcome::Internal {
                path: admitted.relative().clone(),
            },
        })
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
        ResolutionRequest {
            key: ResolutionRequestKey {
                importer: ProjectRelativePath::new("main.js").unwrap(),
                kind: ResolutionRequestKind::StaticImport,
                range: SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap())
                    .unwrap(),
            },
            request: specifier.into(),
        }
    }

    #[test]
    fn delegates_builtin_detection_and_canonicalization_to_oxc() {
        let options = ProjectLoadOptions::default().validated().unwrap();
        let resolver = ProjectResolver::new(
            SourceAdmission::new(Path::new("."), &options).unwrap(),
            &ProjectSelection::entry("main.js"),
        )
        .unwrap();

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
    }

    #[test]
    fn unresolved_bare_packages_remain_external() {
        let options = ProjectLoadOptions::default().validated().unwrap();
        let resolver = ProjectResolver::new(
            SourceAdmission::new(Path::new("."), &options).unwrap(),
            &ProjectSelection::entry("main.js"),
        )
        .unwrap();

        assert_eq!(
            resolver.resolve(&request("not-a-node-builtin")).unwrap(),
            ResolverOutcome::External {
                package: PackageSpecifier::new("not-a-node-builtin").unwrap(),
            }
        );
    }
}
