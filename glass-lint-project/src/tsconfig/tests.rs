#[allow(unused_imports)]
use std::{path::Path, time::Instant};

#[allow(unused_imports)]
use super::{
    selection::{
        CompiledTsconfigSelection, MergedSelection, ParentSelection, TsconfigPatternSet,
        merge_selection,
    },
    *,
};
#[allow(unused_imports)]
use crate::tests::TempProject;

fn merge_same(child: ParsedTsconfig, parent: Option<MergedSelection>) -> MergedSelection {
    let dir = Path::new(".");
    let parent = parent.map(|selection| ParentSelection::new(selection, dir.to_path_buf()));
    merge_selection(child, parent, dir)
}

fn default_budget() -> ConfigTraversalBudget {
    ConfigTraversalBudget::default()
}

fn default_resource_budget() -> ProjectResourceBudget {
    ProjectResourceBudget::new(250_000, 512 * 1024 * 1024)
}

fn build_effective_config(
    config_path: &Path,
    fallback_base: &Path,
    deadline: Option<Instant>,
    diagnostics: &mut Vec<TsconfigDiagnostic>,
    budget: ConfigTraversalBudget,
    config_count: &mut usize,
    resource_budget: &mut ProjectResourceBudget,
) -> Result<(CompiledTsconfigSelection, Vec<ReferenceEntry>), crate::error::ProjectLoadError> {
    let mut traversal =
        TsconfigTraversal::new(deadline, diagnostics, budget, config_count, resource_budget);
    traversal.build_effective_config(config_path, fallback_base)
}

mod budgets;
mod parsing;
mod rebasing;
mod selection;
