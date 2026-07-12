//! Provenance-aware, declarative JavaScript API matching.

use swc_ecma_ast::Program;

mod result;
mod rule;
mod semantic;

pub use result::{ApiCapability, ApiClassificationResult};
pub use rule::{
    ApiCatalogError, ApiCategory, ApiMatcher, ApiRule, ApiRuleBuildError, ApiRuleBuilder,
    ApiSeverity, CallMatcher, ClassMatcher, Confidence, ConstructorMatcher, FlowMatcher,
    FlowValueMatcher, InstanceMemberCallMatcher, Matcher, MemberCallMatcher, MemberReadMatcher,
    ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
};

/// A pre-compiled, normalized matcher for one catalog rule.
#[derive(Debug, Clone)]
pub(crate) struct CompiledRule {
    #[allow(dead_code)]
    pub(crate) catalog_index: usize,
    pub(crate) matcher: ApiMatcher,
}

/// A one-time compiled catalog of matchers, built once at `Linter`
/// construction and reused for every `lint()` call.
#[derive(Debug, Clone)]
pub(crate) struct CompiledCatalog {
    pub(crate) rules: Vec<CompiledRule>,
}

impl CompiledCatalog {
    pub(crate) fn from_rules(rules: &[ApiRule]) -> Self {
        let compiled = rules
            .iter()
            .enumerate()
            .map(|(index, rule)| CompiledRule {
                catalog_index: index,
                matcher: rule.matcher_for_compilation(),
            })
            .collect();
        Self { rules: compiled }
    }
}

/// Classifies a parsed JavaScript program with caller-provided rules.
///
/// The program must already have parsed successfully.  Strict matchers use
/// lexical scope, declaration timing, aliases, and module provenance; dynamic
/// or unsupported behavior resolves to unknown.  The returned evidence is
/// source ordered, deduplicated, and bounded to 16 occurrences per rule.
#[allow(dead_code)]
pub fn classify_api_usage(program: &Program, rules: &[ApiRule]) -> ApiClassificationResult {
    let catalog = CompiledCatalog::from_rules(rules);
    let selected = (0..rules.len()).collect::<Vec<_>>();
    classify_compiled_api_usage(program, &catalog, rules, &selected)
}

/// Classify using a pre-compiled catalog.  The `rules` slice provides
/// metadata for the result; the `catalog` provides the matchers used for
/// analysis.  Both must have the same length and the same catalog order.
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
    let semantic = semantic::SemanticModel::analyze_compiled(program, &matchers);
    let mut result = ApiClassificationResult::default();

    for &index in selected {
        let Some(rule) = rules.get(index) else {
            continue;
        };
        let evidence = semantic.evidence_for(index);
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
