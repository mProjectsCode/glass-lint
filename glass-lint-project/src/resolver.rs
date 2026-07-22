//! Oxc-backed module resolution and provider-neutral result classification.

use std::path::Path;

use glass_lint_core::{
    ProjectRelativePath, ResolutionRequest, ResolutionRequestKind, ResolverOutcome,
    is_internal_module_request,
};
use oxc_resolver::{ResolveError, ResolveOptions, Resolver};

use crate::{
    admission::{PathAdmission, SourceAdmission, absolute_path},
    error::ProjectLoadError,
    options::{ProjectSelection, ValidatedProjectLoadOptions},
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
        let import = Resolver::new(Self::build_options(
            admission.canonical_root(),
            selection,
            admission.options(),
            false,
        )?);
        let require = import.clone_with_options(Self::build_options(
            admission.canonical_root(),
            selection,
            admission.options(),
            true,
        )?);
        Ok(Self {
            admission,
            import,
            require,
        })
    }

    fn build_options(
        root: &Path,
        selection: &ProjectSelection,
        options: &ValidatedProjectLoadOptions,
        require: bool,
    ) -> Result<ResolveOptions, ProjectLoadError> {
        let extension_alias = options
            .extension_aliases()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut resolver_options = ResolveOptions {
            condition_names: if require {
                vec!["node".into(), "require".into()]
            } else {
                vec!["node".into(), "import".into()]
            },
            extensions: options.extensions().map(str::to_owned).collect(),
            extension_alias,
            symlinks: options.follow_symlinks(),
            roots: vec![root.to_path_buf()],
            builtin_modules: true,
            ..ResolveOptions::default()
        };
        if let ProjectSelection::Tsconfig(path) = selection {
            resolver_options.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Manual(
                oxc_resolver::TsconfigOptions {
                    config_file: absolute_path(path)?,
                    references: oxc_resolver::TsconfigReferences::Auto,
                },
            ));
        }
        Ok(resolver_options)
    }

    /// Resolve one request into a provider-neutral, root-classified outcome.
    pub fn resolve(&self, request: &ResolutionRequest) -> ResolverOutcome {
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
            Err(ResolveError::Builtin { resolved, .. }) => {
                ResolverOutcome::Builtin { name: resolved }
            }
            Err(_) if is_internal_module_request(&request.request) => ResolverOutcome::Missing,
            Err(_) => ResolverOutcome::External {
                package: package_name(&request.request),
            },
        }
    }

    fn classify(&self, request: &str, path: &Path) -> ResolverOutcome {
        let Ok(admission) = self.admission.classify(path) else {
            return ResolverOutcome::Missing;
        };
        let internal = is_internal_module_request(request);
        match admission {
            PathAdmission::Outside(path) => {
                if internal {
                    ResolverOutcome::OutsideProject {
                        path: path.as_ref().to_string_lossy().into_owned(),
                    }
                } else {
                    ResolverOutcome::External {
                        package: package_name(request),
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
                        package: package_name(request),
                    }
                }
            }
            PathAdmission::Unsupported(path) => ResolverOutcome::Unsupported {
                reason: format!("unsupported target `{}`", path.as_ref().display()),
            },
            PathAdmission::Admitted(path) => {
                let relative = path
                    .as_ref()
                    .strip_prefix(self.admission.canonical_root())
                    .unwrap_or_else(|_| path.as_ref())
                    .to_string_lossy()
                    .replace('\\', "/");
                let Ok(path) = ProjectRelativePath::new(&relative) else {
                    return ResolverOutcome::Unsupported {
                        reason: format!("invalid normalized target `{relative}`"),
                    };
                };
                ResolverOutcome::Internal { path }
            }
        }
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
    use glass_lint_core::{Position, ProjectRelativePath, ResolutionRequestKey, SourceRange};

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
                resolver.resolve(&request(specifier)),
                ResolverOutcome::Builtin {
                    name: expected.into()
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
            resolver.resolve(&request("not-a-node-builtin")),
            ResolverOutcome::External {
                package: "not-a-node-builtin".into()
            }
        );
    }
}
