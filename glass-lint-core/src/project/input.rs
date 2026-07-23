//! Normalization and validation of the public project input contract.
//!
//! The staged project session owns the canonical normalization pipeline via
//! [`crate::project::session`]. The functions here are shared utilities used
//! by the session, types, and CLI loading code.

use std::path::{Path, PathBuf};

use crate::project::{
    ProjectInputError, ProjectRelativePath, ResolutionRequestKey, ResolverOutcome,
};

/// Validate the root path that anchors project-relative normalization.
pub fn normalize_root(path: &Path) -> Result<PathBuf, ProjectInputError> {
    if path.as_os_str().is_empty() {
        Err(ProjectInputError::InvalidPath(String::new()))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Normalize a project-relative path and reject escapes/absolute paths.
pub fn normalize_relative(path: impl AsRef<str>) -> Result<ProjectRelativePath, ProjectInputError> {
    let original = path.as_ref().to_string();
    let path = path.as_ref().replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.split('/').any(|part| part == "..")
    {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        Err(ProjectInputError::InvalidPath(original))
    } else {
        Ok(ProjectRelativePath::from_normalized(parts.join("/")))
    }
}

/// Normalize an explicitly outside-project target without losing absoluteness.
pub fn normalize_outside_target(path: &str) -> Result<String, ProjectInputError> {
    let original = path.to_string();
    let path = path.replace('\\', "/");
    if path.is_empty() || path.contains('\0') {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if absolute {
                continue;
            }
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
        } else {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err(ProjectInputError::InvalidPath(original));
    }
    Ok(if absolute {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    })
}

/// Normalize and validate one typed resolver result.
pub fn normalize_result(result: &mut ResolverOutcome) -> Result<(), ProjectInputError> {
    match result {
        ResolverOutcome::Internal { path } => *path = normalize_relative(path.as_str())?,
        ResolverOutcome::OutsideProject { path } => *path = normalize_outside_target(path)?,
        ResolverOutcome::External { package } if package.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(package.clone()));
        }
        ResolverOutcome::Builtin { name } if name.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(name.clone()));
        }
        ResolverOutcome::Unsupported { reason } if reason.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(reason.clone()));
        }
        _ => {}
    }
    Ok(())
}

/// Normalize an importer/range key and enforce one-based ordered positions.
pub fn normalize_resolution_key(key: &mut ResolutionRequestKey) -> Result<(), ProjectInputError> {
    key.importer = normalize_relative(key.importer.as_str())?;
    Ok(())
}
