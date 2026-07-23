//! Filesystem membership and source loading.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    admission::{AdmittedSourcePath, CanonicalProjectPath, SourceAdmission},
    error::ProjectLoadError,
    options::ProjectSelection,
    tsconfig::{self, Tsconfig, TsconfigDiagnostic},
    walk,
};

/// Discovers the bounded set of source files that belongs to a selection.
pub struct ProjectDiscovery<'adm, 'opt> {
    admission: &'adm SourceAdmission<'opt>,
    deadline: Option<Instant>,
}

pub struct DiscoveryResult {
    pub paths: Vec<AdmittedSourcePath>,
    pub diagnostics: Vec<TsconfigDiagnostic>,
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
    ) -> Result<DiscoveryResult, ProjectLoadError> {
        let (mut paths, diagnostics) = match selection {
            ProjectSelection::Entry(_) => (self.entry_path(selection_path)?, Vec::new()),
            ProjectSelection::Directory(_) => (self.discover(selection_path)?, Vec::new()),
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
        Ok(DiscoveryResult { paths, diagnostics })
    }

    fn entry_path(&self, path: &Path) -> Result<Vec<AdmittedSourcePath>, ProjectLoadError> {
        if !path.is_file() {
            return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
        }
        if !self.admission.supports(path) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        match self.admission.classify(path)? {
            crate::admission::PathAdmission::Admitted(path) => Ok(vec![path]),
            crate::admission::PathAdmission::Outside(path) => {
                Err(ProjectLoadError::SelectionOutsideRoot {
                    selection: path.into_path_buf(),
                    root: self.admission.canonical_root().to_path_buf(),
                })
            }
            crate::admission::PathAdmission::Excluded(path)
            | crate::admission::PathAdmission::Unsupported(path) => {
                Err(ProjectLoadError::UnsupportedSource(path.into_path_buf()))
            }
        }
    }

    fn validate_membership(
        &self,
        paths: &mut Vec<AdmittedSourcePath>,
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

    fn discover(&self, directory: &Path) -> Result<Vec<AdmittedSourcePath>, ProjectLoadError> {
        let Some(_metadata) = walk::resolve_root(self.admission.options(), directory)? else {
            return Ok(Vec::new());
        };
        walk::collect_files(self.admission, directory, self.deadline, &mut |_| true)
    }

    fn discover_tsconfig(
        &self,
        config: &Path,
        directory: &Path,
    ) -> Result<(Vec<AdmittedSourcePath>, Vec<TsconfigDiagnostic>), ProjectLoadError> {
        let mut visited = BTreeSet::new();
        let mut active = Vec::new();
        let mut selected = BTreeSet::new();
        let mut cycle_diagnostics = Vec::new();
        let canonical_config = self.admission.canonicalize(config)?;
        self.collect_tsconfig(
            &canonical_config,
            directory,
            &mut visited,
            &mut active,
            &mut selected,
            &mut cycle_diagnostics,
        )?;
        // Cycle diagnostics are recorded but the cyclic branch already returns
        // a fail-closed config (empty files, excludes all). Independent branches
        // continue normally.
        cycle_diagnostics.sort_by(|left, right| {
            left.config_path
                .cmp(&right.config_path)
                .then_with(|| left.cycle_target.cmp(&right.cycle_target))
                .then_with(|| left.message.cmp(&right.message))
        });
        cycle_diagnostics.dedup();
        Ok((selected.into_iter().collect(), cycle_diagnostics))
    }

    fn collect_tsconfig(
        &self,
        config: &CanonicalProjectPath,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
        active: &mut Vec<PathBuf>,
        selected: &mut BTreeSet<AdmittedSourcePath>,
        cycle_diagnostics: &mut Vec<TsconfigDiagnostic>,
    ) -> Result<(), ProjectLoadError> {
        let config_path = config.as_ref().to_path_buf();
        if active.contains(&config_path) {
            cycle_diagnostics.push(TsconfigDiagnostic {
                config_path: config_path.clone(),
                cycle_target: Some(config_path),
                message: "cycle detected in project references".into(),
            });
            return Ok(());
        }
        if !visited.insert(config.as_ref().to_path_buf()) {
            return Ok(());
        }

        active.push(config.as_ref().to_path_buf());

        let canonical = config.as_ref().to_path_buf();
        let base = config
            .as_ref()
            .parent()
            .unwrap_or(fallback_directory)
            .to_path_buf();

        // Phase 1-3: Build effective config (typed parse, extends resolution, merge)
        let (effective, references) =
            tsconfig::build_effective_config(&canonical, &base, cycle_diagnostics)?;

        // Phase 4: Select sources using the typed effective config
        self.select_sources(&effective, &base, selected)?;

        // Phase 5: Traverse project references
        let result = self.collect_references_typed(
            &base,
            visited,
            active,
            selected,
            cycle_diagnostics,
            &references,
        );
        active.pop();
        result
    }

    fn select_sources(
        &self,
        config: &Tsconfig,
        base: &Path,
        selected: &mut BTreeSet<AdmittedSourcePath>,
    ) -> Result<(), ProjectLoadError> {
        if let Some(files) = &config.files {
            // Explicit files list
            for file in files {
                let path = base.join(file);
                if path.exists()
                    && self.admission.supports(&path)
                    && let crate::admission::PathAdmission::Admitted(path) =
                        self.admission.classify(&path)?
                {
                    selected.insert(path);
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

    fn collect_references_typed(
        &self,
        base: &Path,
        visited: &mut BTreeSet<PathBuf>,
        active: &mut Vec<PathBuf>,
        selected: &mut BTreeSet<AdmittedSourcePath>,
        cycle_diagnostics: &mut Vec<TsconfigDiagnostic>,
        references: &[tsconfig::ReferenceEntry],
    ) -> Result<(), ProjectLoadError> {
        for reference in references {
            let mut target = base.join(&reference.path);
            if target.is_dir() {
                target = target.join("tsconfig.json");
            }
            if target.exists() {
                let canonical_target = self.admission.canonicalize(&target)?;
                self.collect_tsconfig(
                    &canonical_target,
                    base,
                    visited,
                    active,
                    selected,
                    cycle_diagnostics,
                )?;
            }
        }
        Ok(())
    }
}
