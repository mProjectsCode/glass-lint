//! Filesystem membership and source loading.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    admission::{AdmissionSet, AdmittedSourcePath, CanonicalProjectPath, SourceAdmission},
    error::ProjectLoadError,
    options::ProjectSelection,
    tsconfig::{self, TsconfigDiagnostic},
    walk,
};

/// Discovers the bounded set of source files that belongs to a selection.
///
/// Every discovered file passes through a single shared [`AdmissionSet`] so
/// that the file-count budget is enforced across roots, tsconfig references,
/// and directory walks. Duplicate or overlapping entries do not consume the
/// budget twice.
pub struct ProjectDiscovery<'adm, 'opt> {
    admission: &'adm SourceAdmission<'opt>,
    deadline: Option<Instant>,
    admitted: AdmissionSet,
}

pub struct DiscoveryResult {
    pub paths: Vec<AdmittedSourcePath>,
    pub diagnostics: Vec<TsconfigDiagnostic>,
}

impl<'adm, 'opt> ProjectDiscovery<'adm, 'opt> {
    /// Create a discovery view over a validated admission boundary.
    pub fn with_deadline(
        admission: &'adm SourceAdmission<'opt>,
        deadline: Instant,
        max_files: usize,
    ) -> Self {
        Self {
            admission,
            deadline: Some(deadline),
            admitted: AdmissionSet::new(max_files),
        }
    }

    /// Resolve a selection into sorted, root-contained initial source paths.
    pub fn initial_paths(
        mut self,
        selection: &ProjectSelection,
        selection_path: &Path,
    ) -> Result<DiscoveryResult, ProjectLoadError> {
        let diagnostics = match selection {
            ProjectSelection::Entry(_) => {
                self.entry_path(selection_path)?;
                Vec::new()
            }
            ProjectSelection::Directory(_) => {
                self.discover(selection_path)?;
                Vec::new()
            }
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

        let paths = self.admitted.into_admitted_paths();
        Ok(DiscoveryResult { paths, diagnostics })
    }

    fn entry_path(&mut self, path: &Path) -> Result<(), ProjectLoadError> {
        if !path.is_file() {
            return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
        }
        if !self.admission.supports(path) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        match self.admission.classify(path)? {
            crate::admission::PathAdmission::Admitted(admitted) => {
                self.admitted.admit(&admitted)?;
                Ok(())
            }
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

    fn discover(&mut self, directory: &Path) -> Result<(), ProjectLoadError> {
        let Some(_metadata) = walk::resolve_root(self.admission.options(), directory)? else {
            return Ok(());
        };
        walk::collect_files(
            self.admission,
            directory,
            self.deadline,
            &mut |_| true,
            &mut self.admitted,
        )
    }

    fn discover_tsconfig(
        &mut self,
        config: &Path,
        directory: &Path,
    ) -> Result<Vec<TsconfigDiagnostic>, ProjectLoadError> {
        let mut visited = BTreeSet::new();
        let mut active = Vec::new();
        let mut cycle_diagnostics = Vec::new();
        let canonical_config = SourceAdmission::canonicalize(config)?;
        self.collect_tsconfig(
            &canonical_config,
            directory,
            &mut visited,
            &mut active,
            &mut cycle_diagnostics,
        )?;
        cycle_diagnostics.sort_by(|left, right| {
            left.config_path
                .cmp(&right.config_path)
                .then_with(|| left.cycle_target.cmp(&right.cycle_target))
                .then_with(|| left.message.cmp(&right.message))
        });
        cycle_diagnostics.dedup();
        Ok(cycle_diagnostics)
    }

    fn collect_tsconfig(
        &mut self,
        config: &CanonicalProjectPath,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
        active: &mut Vec<PathBuf>,
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
            tsconfig::build_effective_config(&canonical, &base, self.deadline, cycle_diagnostics)?;

        // Phase 4: Select sources using the typed effective config, admitting
        // into the shared budget.
        self.select_sources(&effective, &base)?;

        // Phase 5: Traverse project references
        let result =
            self.collect_references_typed(&base, visited, active, cycle_diagnostics, &references);
        active.pop();
        result
    }

    fn select_sources(
        &mut self,
        config: &tsconfig::CompiledTsconfigSelection,
        base: &Path,
    ) -> Result<(), ProjectLoadError> {
        if let Some(files) = &config.files {
            for file in files {
                let path = base.join(file);
                if path.exists()
                    && self.admission.supports(&path)
                    && let crate::admission::PathAdmission::Admitted(admitted) =
                        self.admission.classify(&path)?
                {
                    self.admitted.admit(&admitted)?;
                }
            }
        } else {
            let mut include = |path: &Path| {
                let Ok(relative) = path.strip_prefix(base) else {
                    return false;
                };
                config
                    .pattern_set
                    .is_included(&relative.to_string_lossy().replace('\\', "/"))
            };
            walk::collect_files(
                self.admission,
                base,
                self.deadline,
                &mut include,
                &mut self.admitted,
            )?;
        }
        Ok(())
    }

    fn collect_references_typed(
        &mut self,
        base: &Path,
        visited: &mut BTreeSet<PathBuf>,
        active: &mut Vec<PathBuf>,
        cycle_diagnostics: &mut Vec<TsconfigDiagnostic>,
        references: &[tsconfig::ReferenceEntry],
    ) -> Result<(), ProjectLoadError> {
        for reference in references {
            let mut target = base.join(&reference.path);
            if target.is_dir() {
                target = target.join("tsconfig.json");
            }
            if target.exists() {
                let canonical_target = SourceAdmission::canonicalize(&target)?;
                self.collect_tsconfig(&canonical_target, base, visited, active, cycle_diagnostics)?;
            }
        }
        Ok(())
    }
}
