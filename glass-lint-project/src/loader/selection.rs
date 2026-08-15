use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    boundary::{AcceptedSourcePath, SourceBoundary, absolute_path},
    budget::ProjectResourceBudget,
    discovery::{DiscoveryResult, ProjectDiscovery},
    error::ProjectLoadError,
    options::{ProjectSelection, ValidatedProjectLoadOptions},
    tsconfig,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct LoadDeadline(Instant);

impl LoadDeadline {
    pub(super) fn after_millis(timeout_ms: u64) -> Self {
        Self(Instant::now() + Duration::from_millis(timeout_ms))
    }

    pub(super) fn instant(self) -> Instant {
        self.0
    }

    pub(super) fn check(self) -> Result<(), ProjectLoadError> {
        (Instant::now() <= self.0)
            .then_some(())
            .ok_or(ProjectLoadError::Timeout)
    }
}

/// Canonical absolute paths established before the load loop starts.
pub(super) struct ProjectPaths<'a> {
    pub(super) boundary: SourceBoundary<'a>,
    pub(super) initial_paths: VecDeque<AcceptedSourcePath>,
    pub(super) diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
}

impl<'a> ProjectPaths<'a> {
    pub(super) fn from_selection(
        options: &'a ValidatedProjectLoadOptions,
        selection: &ProjectSelection,
        deadline: Instant,
        budget: &mut ProjectResourceBudget,
    ) -> Result<Self, ProjectLoadError> {
        let selection_path = absolute_path(selection.path())?;
        if !selection_path.exists() {
            return Err(ProjectLoadError::SelectionNotFound(selection_path));
        }
        let root = project_root(options, selection, &selection_path)?;
        let boundary = SourceBoundary::new(&root, options)?;
        let canonical_selection = SourceBoundary::canonicalize(&selection_path)?;
        if !boundary.is_inside_root(canonical_selection.as_ref()) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: canonical_selection.into_path_buf(),
                root,
            });
        }
        let discover = ProjectDiscovery::with_deadline(
            &boundary,
            deadline,
            options.max_files(),
            tsconfig::ConfigTraversalBudget::new(
                options.max_config_count(),
                options.max_config_depth(),
            ),
            budget,
        );
        let DiscoveryResult { paths, diagnostics } =
            discover.initial_paths(selection, canonical_selection.as_ref())?;
        Ok(Self {
            boundary,
            initial_paths: paths.into(),
            diagnostics,
        })
    }
}

fn project_root(
    options: &ValidatedProjectLoadOptions,
    selection: &ProjectSelection,
    path: &Path,
) -> Result<PathBuf, ProjectLoadError> {
    if let Some(root) = options.root() {
        return absolute_path(root);
    }
    Ok(match selection {
        ProjectSelection::Directory(_) => path.to_path_buf(),
        ProjectSelection::Entry(_) | ProjectSelection::Tsconfig(_) => {
            path.parent().unwrap_or(path).to_path_buf()
        }
    })
}
