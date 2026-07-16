//! Oxc-backed module resolution and provider-neutral result classification.

use std::path::{Path, PathBuf};

use glass_lint_core::{ResolutionRequest, ResolutionRequestKind, ResolutionResult};
use oxc_resolver::{ResolveError, ResolveOptions, Resolver};

use crate::{
    discovery::{absolute_path, excluded_path, inside_root, realpath, supported_path},
    options::{ProjectLoadOptions, ProjectSelection},
};

/// Keeps import and CommonJS resolution policy together for one project.
pub struct ProjectResolver {
    root: PathBuf,
    options: ProjectLoadOptions,
    import: Resolver,
    require: Resolver,
}

impl ProjectResolver {
    pub fn new(root: &Path, selection: &ProjectSelection, options: &ProjectLoadOptions) -> Self {
        let import = Resolver::new(resolver_options(root, selection, options, false));
        let require = import.clone_with_options(resolver_options(root, selection, options, true));
        Self {
            root: root.to_path_buf(),
            options: options.clone(),
            import,
            require,
        }
    }

    pub fn resolve(&self, request: &ResolutionRequest) -> ResolutionResult {
        let importer = self.root.join(&request.key.importer);
        let directory = importer.parent().unwrap_or(&self.root);
        let resolver = if request.key.kind == ResolutionRequestKind::Require {
            &self.require
        } else {
            &self.import
        };
        match resolver.resolve(directory, &request.request) {
            Ok(resolution) => self.classify(&request.request, resolution.path()),
            Err(ResolveError::Builtin { resolved, .. }) => {
                ResolutionResult::Builtin { name: resolved }
            }
            Err(_) if is_internal_request(&request.request) => ResolutionResult::Missing,
            Err(_) => ResolutionResult::External {
                package: package_name(&request.request),
            },
        }
    }

    fn classify(&self, request: &str, path: &Path) -> ResolutionResult {
        let Ok(path) = realpath(path) else {
            return ResolutionResult::Missing;
        };
        if !inside_root(&self.root, &path) {
            return if is_internal_request(request) {
                ResolutionResult::OutsideProject {
                    path: path.to_string_lossy().into_owned(),
                }
            } else {
                ResolutionResult::External {
                    package: package_name(request),
                }
            };
        }
        if excluded_path(&self.root, &path, &self.options.excluded_directories) {
            return if is_internal_request(request) {
                ResolutionResult::Unsupported {
                    reason: format!("excluded target `{}`", path.display()),
                }
            } else {
                ResolutionResult::External {
                    package: package_name(request),
                }
            };
        }
        if !supported_path(&path, &self.options.extensions) {
            return ResolutionResult::Unsupported {
                reason: format!("unsupported target `{}`", path.display()),
            };
        }
        let relative = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        ResolutionResult::Internal { path: relative }
    }
}

fn resolver_options(
    root: &Path,
    selection: &ProjectSelection,
    options: &ProjectLoadOptions,
    require: bool,
) -> ResolveOptions {
    let extension_alias = options
        .extension_aliases
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut resolver_options = ResolveOptions {
        condition_names: if require {
            vec!["node".into(), "require".into()]
        } else {
            vec!["node".into(), "import".into()]
        },
        extensions: options.extensions.clone(),
        extension_alias,
        symlinks: options.follow_symlinks,
        roots: vec![root.to_path_buf()],
        builtin_modules: true,
        ..ResolveOptions::default()
    };
    if let ProjectSelection::TsConfig(path) = selection {
        resolver_options.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Manual(
            oxc_resolver::TsconfigOptions {
                config_file: absolute_path(path),
                references: oxc_resolver::TsconfigReferences::Auto,
            },
        ));
    }
    resolver_options
}

fn is_internal_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
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
    use glass_lint_core::{Position, ResolutionRequestKey, SourceRange};

    use super::*;

    fn request(specifier: &str) -> ResolutionRequest {
        ResolutionRequest {
            key: ResolutionRequestKey {
                importer: "main.js".into(),
                kind: ResolutionRequestKind::Import,
                range: SourceRange {
                    start: Position { line: 1, column: 1 },
                    end: Position { line: 1, column: 2 },
                },
            },
            request: specifier.into(),
        }
    }

    #[test]
    fn delegates_builtin_detection_and_canonicalization_to_oxc() {
        let options = ProjectLoadOptions::default();
        let resolver = ProjectResolver::new(
            Path::new("."),
            &ProjectSelection::entry("main.js"),
            &options,
        );

        for (specifier, expected) in [
            ("fs", "node:fs"),
            ("node:fs", "node:fs"),
            ("assert/strict", "node:assert/strict"),
            ("timers/promises", "node:timers/promises"),
        ] {
            assert_eq!(
                resolver.resolve(&request(specifier)),
                ResolutionResult::Builtin {
                    name: expected.into()
                },
                "specifier: {specifier}"
            );
        }
    }

    #[test]
    fn unresolved_bare_packages_remain_external() {
        let options = ProjectLoadOptions::default();
        let resolver = ProjectResolver::new(
            Path::new("."),
            &ProjectSelection::entry("main.js"),
            &options,
        );

        assert_eq!(
            resolver.resolve(&request("not-a-node-builtin")),
            ResolutionResult::External {
                package: "not-a-node-builtin".into()
            }
        );
    }
}
