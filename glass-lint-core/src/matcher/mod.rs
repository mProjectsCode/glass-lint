//! Provenance-aware, declarative JavaScript API matching.

use swc_ecma_ast::Program;

mod result;
mod rule;
mod semantic;

pub use result::{ApiCapability, ApiClassificationResult};
pub use rule::{
    ApiCatalogError, ApiCategory, ApiRule, ApiRuleBuildError, ApiRuleBuilder, ApiSeverity,
    CallMatcher, ClassMatcher, Confidence, ConstructorMatcher, FlowMatcher, FlowValueMatcher,
    InstanceMemberCallMatcher, Matcher, MemberCallMatcher, MemberReadMatcher,
    ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
};

/// Classifies a parsed program with caller-provided rules. Core owns no catalog.
pub fn classify_api_usage(program: Option<&Program>, rules: &[ApiRule]) -> ApiClassificationResult {
    let semantic = semantic::SemanticModel::analyze(program, rules);
    let mut result = ApiClassificationResult::default();

    for (index, rule) in rules.iter().enumerate() {
        let evidence = semantic.evidence_for(index, rule);
        if evidence.is_empty() {
            continue;
        }

        result.capabilities.push(ApiCapability {
            id: rule.id.clone(),
            label: rule.label.clone(),
            category: rule.category.clone(),
            severity: rule.severity,
            confidence: rule.confidence,
            evidence,
        });
    }

    result
}

/// Validates catalog-wide invariants that are independent of a provider.
pub fn validate_catalog(rules: &[ApiRule]) -> Result<(), ApiCatalogError> {
    let mut ids = std::collections::BTreeSet::new();
    for rule in rules {
        if !ids.insert(rule.id.clone()) {
            return Err(ApiCatalogError::DuplicateRule(rule.id.clone()));
        }
    }
    Ok(())
}
