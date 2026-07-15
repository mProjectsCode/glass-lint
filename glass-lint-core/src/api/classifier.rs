use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::ProjectSemanticModel;
use crate::api::{
    classification::ApiClassificationResult, compiler::CompiledCatalog, rule::ApiRule,
};
use crate::project::ModuleId;

pub(crate) fn classify_compiled_api_usage(
    project: &ProjectSemanticModel,
    catalog: &CompiledCatalog,
    rules: &[ApiRule],
    selected: &BTreeSet<usize>,
) -> BTreeMap<ModuleId, ApiClassificationResult> {
    debug_assert_eq!(catalog.rules.len(), rules.len());
    crate::analysis::classify_project(project, catalog, rules, selected)
}
