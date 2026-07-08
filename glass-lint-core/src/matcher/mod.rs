//! Provenance-aware, declarative JavaScript API matching.

use swc_ecma_ast::Program;

mod result;
mod rule;
mod symbol_index;

pub use result::{ApiCapability, ApiClassificationResult, Disclosure};
pub use rule::{ApiCatalogError, ApiCategory, ApiRule, ApiRuleBuildError, ApiSeverity, Confidence};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str) -> rule::ApiRuleBuilder {
        ApiRule::builder(id)
            .label(id)
            .category("test")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
    }

    #[test]
    fn resolves_module_provenance_and_rejects_local_lookalikes() {
        let parsed = crate::parse(
            "import { send as sdkSend } from 'example-sdk'; sdkSend(); function send() {} send();",
            "input.js",
        )
        .unwrap();
        let rules = [rule("test.module")
            .module_calls("example-sdk", ["send"])
            .build()
            .unwrap()];
        let result = classify_api_usage(Some(&parsed.program), &rules);
        assert!(result.has_capability("test.module"));
        assert_eq!(result.capabilities()[0].evidence()[0].count(), 1);
    }

    #[test]
    fn follows_rooted_aliases_and_reassignment_order() {
        let parsed = crate::parse(
            "const files = host.files; files.read(); files = local; files.read();",
            "input.js",
        )
        .unwrap();
        let rules = [rule("test.alias")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap()];
        let result = classify_api_usage(Some(&parsed.program), &rules);
        assert!(result.has_capability("test.alias"));
        assert_eq!(result.capabilities()[0].evidence()[0].count(), 1);
    }
}
