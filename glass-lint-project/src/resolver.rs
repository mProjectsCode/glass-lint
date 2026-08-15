//! Oxc-backed module resolution and provider-neutral result classification.

use std::path::Path;

use glass_lint_core::project::{
    BuiltinModuleName, NormalizedOutsidePath, PackageSpecifier, ResolutionRequest,
    ResolutionRequestKind, ResolvedTargetKind, ResolverOutcome, is_internal_module_request,
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
            Err(ResolveError::Builtin { resolved, .. }) => {
                Ok(ResolverOutcome::Target(ResolvedTargetKind::Builtin {
                    name: BuiltinModuleName::new(resolved)?,
                }))
            }
            // Deliberate not-found: bare packages remain external, internal
            // requests become missing.
            Err(ResolveError::NotFound(_) | ResolveError::MatchedAliasNotFound(..))
                if is_internal_module_request(request.specifier()) =>
            {
                Ok(ResolverOutcome::Target(ResolvedTargetKind::Missing))
            }
            Err(ResolveError::NotFound(_) | ResolveError::MatchedAliasNotFound(..)) => Ok(
                external_outcome(request.specifier(), " in not-found request"),
            ),
            // All other resolver errors (I/O, specifier, config, etc.) are
            // operational or invalid — fail closed as unsupported.
            Err(other) => Ok(ResolverOutcome::Target(ResolvedTargetKind::Unsupported {
                reason: format!("resolution failed: {other}"),
            })),
        }
    }

    fn classify(&self, request: &str, path: &Path) -> Result<ResolverOutcome, ProjectLoadError> {
        let classification = self.boundary.classify(path)?;
        let internal = is_internal_module_request(request);
        Ok(match classification {
            PathClassification::Outside(path) => {
                if internal {
                    ResolverOutcome::Target(ResolvedTargetKind::OutsideProject {
                        path: NormalizedOutsidePath::new(
                            path.as_ref().to_string_lossy().into_owned(),
                        )?,
                    })
                } else {
                    external_outcome(request, "")
                }
            }
            PathClassification::Excluded(path) => {
                if internal {
                    ResolverOutcome::Target(ResolvedTargetKind::Unsupported {
                        reason: format!("excluded target `{}`", path.as_ref().display()),
                    })
                } else {
                    external_outcome(request, "")
                }
            }
            PathClassification::Unsupported(path) => {
                ResolverOutcome::Target(ResolvedTargetKind::Unsupported {
                    reason: format!("unsupported target `{}`", path.as_ref().display()),
                })
            }
            PathClassification::Accepted(accepted) => ResolverOutcome::Internal {
                path: accepted.relative().clone(),
            },
        })
    }
}

fn external_outcome(request: &str, context: &str) -> ResolverOutcome {
    match PackageSpecifier::new(package_name(request)) {
        Ok(package) => ResolverOutcome::Target(ResolvedTargetKind::External { package }),
        Err(error) => ResolverOutcome::Target(ResolvedTargetKind::Unsupported {
            reason: format!("invalid package specifier{context}: {error}"),
        }),
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
mod tests;
