//! Normalization and validation of the public project input contract.
//!
//! The staged project session owns the canonical normalization pipeline via
//! The private project-session module owns the canonical normalization
//! pipeline. The functions here are shared utilities used
//! by the session, types, and CLI loading code.

use crate::project::{
    BuiltinModuleName, NormalizedOutsidePath, PackageSpecifier, ProjectInputError,
    ProjectRelativePath, ResolutionRequestKey, ResolverOutcome,
};

/// Whether a normalized (backslash → slash) path is in an absolute form:
/// POSIX root (`/`), drive prefix (`C:/`, `D:/`), or UNC prefix (`//`).
fn is_absolute_form(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with('/') {
        return true;
    }
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Whether a path segment looks like a Windows drive prefix (`C:`, `D:`).
fn is_drive_prefix(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Normalize a project-relative path and reject escapes/absolute paths.
pub fn normalize_relative(path: impl AsRef<str>) -> Result<ProjectRelativePath, ProjectInputError> {
    let original = path.as_ref().to_string();
    let path = path.as_ref().replace('\\', "/");
    if path.is_empty()
        || is_absolute_form(&path)
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
    let absolute = is_absolute_form(&path);
    let had_leading_slash = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if absolute {
                if parts
                    .last()
                    .is_some_and(|last| *last != ".." && !is_drive_prefix(last))
                {
                    parts.pop();
                }
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
    Ok(if had_leading_slash {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    })
}

/// Normalize and validate one typed resolver result.
pub fn normalize_result(result: &mut ResolverOutcome) -> Result<(), ProjectInputError> {
    match result {
        ResolverOutcome::Internal { path } => *path = normalize_relative(path.as_str())?,
        ResolverOutcome::External { package } => {
            *package = PackageSpecifier::new(package.as_str())?;
        }
        ResolverOutcome::Builtin { name } => {
            *name = BuiltinModuleName::new(name.as_str())?;
        }
        ResolverOutcome::OutsideProject { path } => {
            let normalized = normalize_outside_target(path.as_str())?;
            *path = NormalizedOutsidePath::from_validated(normalized);
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
