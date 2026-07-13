use std::collections::BTreeSet;

use crate::Environment;
use crate::analysis::SemanticModel;
use crate::api::{
    classification::ApiClassificationResult, compiler::CompiledCatalog, rule::ApiRule,
};
use swc_ecma_ast::Program;

pub(crate) fn classify_compiled_api_usage(
    program: &Program,
    catalog: &CompiledCatalog,
    rules: &[ApiRule],
    selected: &BTreeSet<usize>,
    environment: &Environment,
) -> ApiClassificationResult {
    debug_assert_eq!(catalog.rules.len(), rules.len());

    let semantic =
        SemanticModel::analyze_compiled(program, catalog.to_matcher_catalog(selected), environment);

    let mut result = ApiClassificationResult::default();
    for rule_index in 0..rules.len() {
        if !selected.contains(&rule_index) {
            continue;
        }
        let Some(rule) = rules.get(rule_index) else {
            continue;
        };

        let evidence = semantic.evidence_for(rule_index);
        if evidence.is_empty() {
            continue;
        }

        result
            .capabilities
            .push(crate::api::classification::ApiCapability {
                id: rule.id().to_string(),
                label: rule.label().to_string(),
                category: rule.category().clone(),
                severity: rule.severity(),
                confidence: rule.confidence(),
                evidence,
            });
    }
    result
}
