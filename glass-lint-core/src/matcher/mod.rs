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

/// Classifies a parsed JavaScript program with caller-provided rules.
///
/// The program must already have parsed successfully.  Strict matchers use
/// lexical scope, declaration timing, aliases, and module provenance; dynamic
/// or unsupported behavior resolves to unknown.  The returned evidence is
/// source ordered, deduplicated, and bounded to 16 occurrences per rule.
pub fn classify_api_usage(program: &Program, rules: &[ApiRule]) -> ApiClassificationResult {
    let semantic = semantic::SemanticModel::analyze(program, rules);
    let mut result = ApiClassificationResult::default();

    for (index, rule) in rules.iter().enumerate() {
        let evidence = semantic.evidence_for(index, rule);
        if evidence.is_empty() {
            continue;
        }

        result.capabilities.push(ApiCapability {
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

/// Validates catalog-wide invariants that are independent of a provider.
pub fn validate_catalog(rules: &[ApiRule]) -> Result<(), ApiCatalogError> {
    let mut ids = std::collections::BTreeSet::new();
    for rule in rules {
        if !ids.insert(rule.id().to_string()) {
            return Err(ApiCatalogError::DuplicateRule(rule.id().to_string()));
        }
    }
    Ok(())
}
