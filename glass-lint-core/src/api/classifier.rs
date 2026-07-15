use std::collections::{BTreeMap, BTreeSet};

use crate::{
    analysis::ProjectSemanticModel,
    api::{classification::ApiClassificationResult, compiler::CompiledCatalog, rule::ApiRule},
    project::ModuleId,
};

// TODO: really don't like this wrapper, just inline it into the two callers
pub(crate) fn classify_compiled_api_usage(
    project: &ProjectSemanticModel,
    catalog: &CompiledCatalog,
    rules: &[ApiRule],
    selected: &BTreeSet<usize>,
) -> BTreeMap<ModuleId, ApiClassificationResult> {
    debug_assert_eq!(catalog.rules.len(), rules.len());
    project.classify(catalog, rules, selected)
}
