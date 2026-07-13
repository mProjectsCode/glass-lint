use crate::api::{
    classification::ApiClassificationResult, compiler::CompiledCatalog, rule::ApiRule,
};
use swc_ecma_ast::Program;

#[allow(dead_code)]
pub fn classify_api_usage(program: &Program, rules: &[ApiRule]) -> ApiClassificationResult {
    let catalog = CompiledCatalog::from_rules(rules);
    let selected = (0..rules.len()).collect::<Vec<_>>();
    classify_compiled_api_usage(program, &catalog, rules, &selected)
}

pub(crate) fn classify_compiled_api_usage(
    program: &Program,
    catalog: &CompiledCatalog,
    rules: &[ApiRule],
    selected: &[usize],
) -> ApiClassificationResult {
    debug_assert_eq!(catalog.rules.len(), rules.len());
    let matchers = catalog
        .rules
        .iter()
        .map(|rule| &rule.matcher)
        .collect::<Vec<_>>();
    let semantic = crate::analysis::SemanticModel::analyze_compiled(program, &matchers, selected);
    let selected = selected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut result = ApiClassificationResult::default();
    for index in 0..rules.len() {
        if !selected.contains(&index) {
            continue;
        }
        let Some(rule) = rules.get(index) else {
            continue;
        };
        let evidence = semantic.evidence_for(index);
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
