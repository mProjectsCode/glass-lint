//! Filesystem membership and source loading.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    admission::{CanonicalProjectPath, SourceAdmission},
    error::ProjectLoadError,
    options::ProjectSelection,
    tsconfig::{self, CycleDiagnostic, Tsconfig},
    walk,
};

/// Discovers the bounded set of source files that belongs to a selection.
pub struct ProjectDiscovery<'adm, 'opt> {
    admission: &'adm SourceAdmission<'opt>,
    deadline: Option<Instant>,
}

impl<'adm, 'opt> ProjectDiscovery<'adm, 'opt> {
    /// Create a discovery view over a validated admission boundary.
    pub fn with_deadline(admission: &'adm SourceAdmission<'opt>, deadline: Instant) -> Self {
        Self {
            admission,
            deadline: Some(deadline),
        }
    }

    /// Resolve a selection into sorted, root-contained initial source paths.
    pub fn initial_paths(
        &self,
        selection: &ProjectSelection,
        selection_path: &Path,
    ) -> Result<Vec<CanonicalProjectPath>, ProjectLoadError> {
        let mut paths = match selection {
            ProjectSelection::Entry(_) => self.entry_path(selection_path)?,
            ProjectSelection::Directory(_) => self.discover(selection_path)?,
            ProjectSelection::Tsconfig(config) => {
                if !selection_path.is_file() {
                    return Err(ProjectLoadError::SelectionNotFile(
                        selection_path.to_path_buf(),
                    ));
                }
                self.discover_tsconfig(
                    config,
                    selection_path
                        .parent()
                        .unwrap_or_else(|| self.admission.canonical_root()),
                )?
            }
        };

        self.validate_membership(&mut paths, selection_path)?;
        Ok(paths)
    }

    fn entry_path(&self, path: &Path) -> Result<Vec<CanonicalProjectPath>, ProjectLoadError> {
        if !path.is_file() {
            return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
        }
        if !self.admission.supports(path) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        let canonical = self.admission.canonicalize(path)?;
        Ok(vec![canonical])
    }

    fn validate_membership(
        &self,
        paths: &mut Vec<CanonicalProjectPath>,
        selection: &Path,
    ) -> Result<(), ProjectLoadError> {
        let mut outside = false;
        paths.retain(|path| {
            if self.admission.is_inside_root(path.as_ref()) {
                true
            } else {
                outside = true;
                false
            }
        });
        if outside {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: selection.to_path_buf(),
                root: self.admission.canonical_root().to_path_buf(),
            });
        }
        paths.sort();
        paths.dedup();
        if paths.len() > self.admission.options().max_files() {
            return Err(ProjectLoadError::TooManyFiles(
                self.admission.options().max_files(),
            ));
        }
        Ok(())
    }

    fn discover(&self, directory: &Path) -> Result<Vec<CanonicalProjectPath>, ProjectLoadError> {
        let Some(_metadata) = walk::resolve_root(self.admission.options(), directory)? else {
            return Ok(Vec::new());
        };
        walk::collect_files(self.admission, directory, self.deadline, &mut |_| true)
    }

    fn discover_tsconfig(
        &self,
        config: &Path,
        directory: &Path,
    ) -> Result<Vec<CanonicalProjectPath>, ProjectLoadError> {
        let mut visited = BTreeSet::new();
        let mut selected = BTreeSet::new();
        let mut cycle_diagnostics = Vec::new();
        let canonical_config = self.admission.canonicalize(config)?;
        self.collect_tsconfig(
            &canonical_config,
            directory,
            &mut visited,
            &mut selected,
            &mut cycle_diagnostics,
        )?;
        // Cycle diagnostics are recorded but the cyclic branch already returns
        // a fail-closed config (empty files, excludes all). Independent branches
        // continue normally.
        Ok(selected.into_iter().collect())
    }

    fn collect_tsconfig(
        &self,
        config: &CanonicalProjectPath,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
        selected: &mut BTreeSet<CanonicalProjectPath>,
        cycle_diagnostics: &mut Vec<CycleDiagnostic>,
    ) -> Result<(), ProjectLoadError> {
        if !visited.insert(config.as_ref().to_path_buf()) {
            return Ok(());
        }

        let canonical = config.as_ref().to_path_buf();
        let base = config
            .as_ref()
            .parent()
            .unwrap_or(fallback_directory)
            .to_path_buf();

        // Phase 1-3: Build effective config (typed parse, extends resolution, merge)
        let effective = tsconfig::build_effective_config(&canonical, &base, cycle_diagnostics)?;

        // Phase 4: Select sources using the typed effective config
        self.select_sources(&effective, &base, selected)?;

        // Phase 5: Traverse project references
        self.collect_references(&canonical, &base, visited, selected, cycle_diagnostics)
    }

    fn select_sources(
        &self,
        config: &Tsconfig,
        base: &Path,
        selected: &mut BTreeSet<CanonicalProjectPath>,
    ) -> Result<(), ProjectLoadError> {
        if let Some(files) = &config.files {
            // Explicit files list
            for file in files {
                let path = base.join(file);
                if path.exists() && self.admission.supports(&path) {
                    selected.insert(self.admission.canonicalize(&path)?);
                }
            }
        } else {
            // Include/exclude pattern matching via the compiled pattern set
            for canonical in self.discover(base)? {
                let relative = canonical
                    .as_ref()
                    .strip_prefix(base)
                    .unwrap_or_else(|_| canonical.as_ref())
                    .to_string_lossy()
                    .replace('\\', "/");
                if config.pattern_set.is_included(&relative) {
                    selected.insert(canonical);
                }
            }
        }
        Ok(())
    }

    fn collect_references(
        &self,
        config_path: &Path,
        base: &Path,
        visited: &mut BTreeSet<PathBuf>,
        selected: &mut BTreeSet<CanonicalProjectPath>,
        cycle_diagnostics: &mut Vec<CycleDiagnostic>,
    ) -> Result<(), ProjectLoadError> {
        // Re-read config DTO to get typed references
        let dto = tsconfig::read_and_parse(config_path)?;
        for ref_path_str in &dto.references {
            let mut target = base.join(ref_path_str);
            if target.is_dir() {
                target = target.join("tsconfig.json");
            }
            if target.exists() {
                let canonical_target = self.admission.canonicalize(&target)?;
                self.collect_tsconfig(
                    &canonical_target,
                    base,
                    visited,
                    selected,
                    cycle_diagnostics,
                )?;
            }
        }
        Ok(())
    }
}
