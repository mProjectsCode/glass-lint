//! Normalization and validation of the public project input contract.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use super::{ModuleId, ProjectInput, ProjectInputError, ResolutionRequestKey, ResolutionResult};

impl ProjectInput {
    /// Canonicalizes project identities and validates all cross-record
    /// references.
    pub fn validate(mut self) -> Result<Self, ProjectInputError> {
        if self.sources.len() > 100_000 {
            return Err(ProjectInputError::BudgetExceeded("source count".into()));
        }
        if self.resolutions.len() > 500_000 {
            return Err(ProjectInputError::BudgetExceeded("resolution count".into()));
        }
        if self
            .sources
            .iter()
            .map(|source| source.source.len())
            .sum::<usize>()
            > 512 * 1024 * 1024
        {
            return Err(ProjectInputError::BudgetExceeded(
                "project source bytes".into(),
            ));
        }
        self.root = normalize_root(&self.root)?;
        let mut sources = BTreeMap::new();
        for mut source in self.sources {
            source.path = normalize_relative(&source.path)?;
            let path = source.path.clone();
            if sources.insert(path.clone(), source).is_some() {
                return Err(ProjectInputError::DuplicateSource(path));
            }
        }
        let source_paths = sources.keys().cloned().collect::<BTreeSet<_>>();
        let mut resolutions = BTreeMap::new();
        for (mut key, mut result) in self.resolutions {
            normalize_resolution_key(&mut key)?;
            if !source_paths.contains(&key.importer) {
                return Err(ProjectInputError::UnknownImporter(key.importer));
            }
            if key.range.start.line == 0
                || key.range.start.column == 0
                || key.range.end.line == 0
                || key.range.end.column == 0
                || key.range.end.line < key.range.start.line
                || (key.range.end.line == key.range.start.line
                    && key.range.end.column < key.range.start.column)
            {
                return Err(ProjectInputError::InvalidRange(key.importer));
            }
            normalize_result(&mut result)?;
            if resolutions.insert(key.clone(), result).is_some() {
                return Err(ProjectInputError::DuplicateResolution(key));
            }
        }
        self.sources = sources.into_values().collect();
        self.resolutions = resolutions.into_iter().collect();
        Ok(self)
    }

    #[must_use]
    pub fn module_ids(&self) -> BTreeMap<String, ModuleId> {
        let mut paths = self
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                (
                    path,
                    ModuleId(u32::try_from(index).expect("module count exceeds ModuleId range")),
                )
            })
            .collect()
    }
}

pub fn normalize_root(path: &Path) -> Result<PathBuf, ProjectInputError> {
    if path.as_os_str().is_empty() {
        Err(ProjectInputError::InvalidPath(String::new()))
    } else {
        Ok(path.to_path_buf())
    }
}

pub fn normalize_relative(path: &str) -> Result<String, ProjectInputError> {
    let original = path.to_string();
    let path = path.replace('\\', "/");
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
        Ok(parts.join("/"))
    }
}

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

pub fn normalize_result(result: &mut ResolutionResult) -> Result<(), ProjectInputError> {
    match result {
        ResolutionResult::Internal { path } => *path = normalize_relative(path)?,
        ResolutionResult::OutsideProject { path } => *path = normalize_outside_target(path)?,
        ResolutionResult::External { package } if package.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(package.clone()));
        }
        ResolutionResult::Builtin { name } if name.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(name.clone()));
        }
        ResolutionResult::Unsupported { reason } if reason.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(reason.clone()));
        }
        _ => {}
    }
    Ok(())
}

pub fn normalize_resolution_key(key: &mut ResolutionRequestKey) -> Result<(), ProjectInputError> {
    key.importer = normalize_relative(&key.importer)?;
    if key.range.start.line == 0
        || key.range.start.column == 0
        || key.range.end.line == 0
        || key.range.end.column == 0
        || key.range.end.line < key.range.start.line
        || (key.range.end.line == key.range.start.line
            && key.range.end.column < key.range.start.column)
    {
        return Err(ProjectInputError::InvalidRange(key.importer.clone()));
    }
    Ok(())
}
