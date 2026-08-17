use super::*;
use crate::api::rule::{
    CompilerInvariantDiagnostic, Confidence, EventQuery, PhysicalPlanDiagnostic, QueryDiagnostic,
    Rule, Severity,
};

fn make_catalog(provider: &str) -> RuleCatalog {
    let rule = Rule::catalog_builder("request")
        .description("Request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    RuleCatalog::new(provider, vec![rule]).unwrap()
}

#[test]
fn combined_catalog_rejects_duplicate_namespaced_ids() {
    let error = RuleCatalog::combine([make_catalog("same"), make_catalog("same")]).unwrap_err();

    assert_eq!(error, RuleId::parse("same:request").unwrap());
}

#[test]
fn combined_catalog_moves_records_without_recompiling() {
    let combined = RuleCatalog::combine([make_catalog("a"), make_catalog("b")]).unwrap();
    assert_eq!(combined.rule_ids().count(), 2);
    assert_eq!(combined.records.len(), 2);
    assert_eq!(
        combined.rule_id(RuleIndex::new(0)).unwrap().as_str(),
        "a:request"
    );
    assert_eq!(
        combined.rule_id(RuleIndex::new(1)).unwrap().as_str(),
        "b:request"
    );
}

#[test]
fn catalog_mapping_preserves_compiler_error_categories() {
    let rule_id = RuleId::parse("test:request").unwrap();
    let cases = [
        (
            CompiledCatalogError::InvalidMatcher {
                rule_id: rule_id.clone(),
                message: "matcher".into(),
            },
            RuleCompilationError::InvalidMatcher("matcher".into()),
        ),
        (
            CompiledCatalogError::InvalidQuery {
                rule_id: rule_id.clone(),
                diagnostic: QueryDiagnostic::new("query", "query".into()),
            },
            RuleCompilationError::InvalidQuery("[query] query".into()),
        ),
        (
            CompiledCatalogError::CompilerInvariant {
                rule_id: rule_id.clone(),
                diagnostic: CompilerInvariantDiagnostic::Internal {
                    detail: "invariant".into(),
                },
            },
            RuleCompilationError::CompilerInvariant("invariant".into()),
        ),
        (
            CompiledCatalogError::InvalidPhysicalPlan {
                rule_id: rule_id.clone(),
                diagnostic: PhysicalPlanDiagnostic::EmptyRoots,
            },
            RuleCompilationError::InvalidPhysicalPlan("physical plan must contain a root".into()),
        ),
    ];

    for (compiled, expected) in cases {
        assert_eq!(
            map_compiled_catalog_error(compiled),
            ProviderCatalogError::InvalidRule(rule_id.clone(), expected)
        );
    }
}
