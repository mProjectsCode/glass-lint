//! Filesystem membership and source loading.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    admission::{AdmissionSet, AdmittedSourcePath, CanonicalProjectPath, SourceAdmission},
    budget::ProjectResourceBudget,
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
pub struct ProjectDiscovery<'adm, 'opt, 'budget> {
    admission: &'adm SourceAdmission<'opt>,
    deadline: Option<Instant>,
    admitted: AdmissionSet,
    config_budget: tsconfig::ConfigTraversalBudget,
    project_budget: &'budget mut ProjectResourceBudget,
}

pub struct DiscoveryResult {
    pub paths: Vec<AdmittedSourcePath>,
    pub diagnostics: Vec<TsconfigDiagnostic>,
}

/// Work item for iterative reference-graph traversal.
struct RefWorkItem {
    config: PathBuf,
    base: PathBuf,
    depth: usize,
}

enum RefStackItem {
    Enter(RefWorkItem),
    Exit,
}

impl<'adm, 'opt, 'budget> ProjectDiscovery<'adm, 'opt, 'budget> {
    /// Create a discovery view over a validated admission boundary.
    pub fn with_deadline(
        admission: &'adm SourceAdmission<'opt>,
        deadline: Instant,
        max_files: usize,
        config_budget: tsconfig::ConfigTraversalBudget,
        project_budget: &'budget mut ProjectResourceBudget,
    ) -> Self {
        Self {
            admission,
            deadline: Some(deadline),
            admitted: AdmissionSet::new(max_files),
            config_budget,
            project_budget,
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
            self.project_budget,
        )
    }

    fn discover_tsconfig(
        &mut self,
        config: &Path,
        directory: &Path,
    ) -> Result<Vec<TsconfigDiagnostic>, ProjectLoadError> {
        let mut cycle_diagnostics = Vec::new();
        let canonical_config = SourceAdmission::canonicalize(config)?;
        self.collect_tsconfig_graph(&canonical_config, directory, &mut cycle_diagnostics)?;
        cycle_diagnostics.sort_by(|left, right| {
            left.config_path
                .cmp(&right.config_path)
                .then_with(|| left.cycle_target.cmp(&right.cycle_target))
                .then_with(|| left.message.cmp(&right.message))
        });
        cycle_diagnostics.dedup();
        Ok(cycle_diagnostics)
    }

    /// Iterative reference-graph traversal with an explicit work stack.
    /// Uses Enter/Exit markers so the `active` set correctly tracks the
    /// current reference chain for cycle detection.
    fn collect_tsconfig_graph(
        &mut self,
        root_config: &CanonicalProjectPath,
        root_directory: &Path,
        cycle_diagnostics: &mut Vec<TsconfigDiagnostic>,
    ) -> Result<(), ProjectLoadError> {
        let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
        let mut active: Vec<PathBuf> = Vec::new();
        let mut config_count = 0usize;
        let mut stack: Vec<RefStackItem> = Vec::new();

        let root_base = Path::new(root_config.as_ref())
            .parent()
            .unwrap_or(root_directory)
            .to_path_buf();
        stack.push(RefStackItem::Enter(RefWorkItem {
            config: root_config.as_ref().to_path_buf(),
            base: root_base,
            depth: 0,
        }));

        while let Some(item) = stack.pop() {
            match item {
                RefStackItem::Exit => {
                    active.pop();
                }
                RefStackItem::Enter(work) => {
                    let config_path = &work.config;

                    // Depth budget check
                    if work.depth >= self.config_budget.max_depth {
                        return Err(ProjectLoadError::ConfigBudgetExhausted {
                            kind: "reference depth",
                            limit: self.config_budget.max_depth,
                        });
                    }

                    // Cycle detection
                    if active.contains(config_path) {
                        cycle_diagnostics.push(TsconfigDiagnostic {
                            config_path: config_path.clone(),
                            cycle_target: Some(config_path.clone()),
                            message: "cycle detected in project references".into(),
                        });
                        continue;
                    }

                    // Already fully visited via another path
                    if !visited.insert(config_path.clone()) {
                        continue;
                    }

                    active.push(config_path.clone());

                    // Config count is tracked inside build_effective_config.
                    // Phase 1-3: Build effective config
                    let (effective, references) = tsconfig::build_effective_config(
                        config_path,
                        &work.base,
                        self.deadline,
                        cycle_diagnostics,
                        self.config_budget,
                        &mut config_count,
                        self.project_budget,
                    )?;

                    // Phase 4: Select sources
                    self.select_sources(&effective, &work.base)?;

                    // Schedule Exit marker so the active stack is cleaned
                    // up after all children have been processed.
                    stack.push(RefStackItem::Exit);

                    // Phase 5: Push references (reverse order so DFS
                    // processes them in their original declaration order).
                    for reference in references.iter().rev() {
                        let mut target = work.base.join(&reference.path);
                        if target.is_dir() {
                            target = target.join("tsconfig.json");
                        }
                        if target.exists() {
                            let canonical_target = SourceAdmission::canonicalize(&target)?;
                            let child_base = Path::new(canonical_target.as_ref())
                                .parent()
                                .map_or_else(|| work.base.clone(), Path::to_path_buf);
                            stack.push(RefStackItem::Enter(RefWorkItem {
                                config: canonical_target.into_path_buf(),
                                base: child_base,
                                depth: work.depth + 1,
                            }));
                        }
                    }
                }
            }
        }

        Ok(())
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
                let relative_str = relative.to_string_lossy();
                config
                    .pattern_set
                    .is_included(&if relative_str.contains('\\') {
                        Cow::Owned(relative_str.replace('\\', "/"))
                    } else {
                        relative_str
                    })
            };
            walk::collect_files(
                self.admission,
                base,
                self.deadline,
                &mut include,
                &mut self.admitted,
                self.project_budget,
            )?;
        }
        Ok(())
    }
}
