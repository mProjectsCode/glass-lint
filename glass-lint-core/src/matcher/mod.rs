//! Provenance-aware, declarative JavaScript API matching.

use swc_ecma_ast::Program;

mod result;
mod rule;
mod symbol_index;

pub use result::{ApiCapability, ApiClassificationResult, Disclosure};
pub use rule::{
    ApiCatalogError, ApiCategory, ApiRule, ApiRuleBuildError, ApiRuleBuilder, ApiSeverity,
    CallMatcher, ClassMatcher, Confidence, ConstructorMatcher, FlowMatcher, FlowValueMatcher,
    Matcher, MemberCallMatcher, MemberReadMatcher,
};

use symbol_index::SymbolIndex;

/// Classifies a parsed program with caller-provided rules. Core owns no catalog.
pub fn classify_api_usage(program: Option<&Program>, rules: &[ApiRule]) -> ApiClassificationResult {
    let aliases = program
        .map(symbol_index::AliasInfo::collect)
        .unwrap_or_default();
    let (symbol_index, argument_evidence) =
        SymbolIndex::collect_for_rules(program, &aliases, rules);
    let mut result = ApiClassificationResult::default();

    for (index, rule) in rules.iter().enumerate() {
        let mut evidence = symbol_index.evidence_for(rule);
        evidence.extend_from_slice(&argument_evidence[index]);
        evidence.truncate(ApiRule::EVIDENCE_LIMIT);
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
        result
            .disclosures
            .extend(rule.implies.iter().map(|id| Disclosure {
                id: id.clone(),
                from_capability: rule.id.clone(),
            }));
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
