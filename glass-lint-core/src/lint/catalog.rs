//! Validated rule catalogs and stable rule-index selection.

use std::{collections::BTreeSet, error::Error, fmt};

use crate::{
    RuleId, RuleMetadata,
    api::{
        classification::RuleIndex,
        compiler::{CompiledRuleRecord, compile_records},
        rule::{CompiledCatalogError, Rule},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Catalog construction failure.
pub enum ProviderCatalogError {
    /// Provider prefix or full rule ID is invalid.
    InvalidRuleId(String),
    /// A rule failed validation or matcher/query compilation.
    InvalidRule(RuleId, RuleCompilationError),
    /// A fully-qualified rule ID occurs in more than one catalog.
    DuplicateRule(RuleId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Categorized compiler failure for one provider rule.
pub enum RuleCompilationError {
    /// The rule used an invalid matcher declaration.
    InvalidMatcher(String),
    /// The authored query could not be compiled.
    InvalidQuery(String),
    /// The compiler encountered an internal invariant failure.
    CompilerInvariant(String),
    /// The normalized query could not form an executable physical plan.
    InvalidPhysicalPlan(String),
}

impl fmt::Display for RuleCompilationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMatcher(message)
            | Self::InvalidQuery(message)
            | Self::CompilerInvariant(message)
            | Self::InvalidPhysicalPlan(message) => f.write_str(message),
        }
    }
}

fn map_compiled_catalog_error(error: CompiledCatalogError) -> ProviderCatalogError {
    let (rule_id, diagnostic) = match error {
        CompiledCatalogError::InvalidMatcher { rule_id, message } => {
            (rule_id, RuleCompilationError::InvalidMatcher(message))
        }
        CompiledCatalogError::InvalidQuery {
            rule_id,
            diagnostic,
        } => (
            rule_id,
            RuleCompilationError::InvalidQuery(diagnostic.to_string()),
        ),
        CompiledCatalogError::CompilerInvariant {
            rule_id,
            diagnostic,
        } => (
            rule_id,
            RuleCompilationError::CompilerInvariant(diagnostic.to_string()),
        ),
        CompiledCatalogError::InvalidPhysicalPlan {
            rule_id,
            diagnostic,
        } => (
            rule_id,
            RuleCompilationError::InvalidPhysicalPlan(diagnostic.to_string()),
        ),
    };
    ProviderCatalogError::InvalidRule(rule_id, diagnostic)
}

impl fmt::Display for ProviderCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRuleId(id) => write!(f, "invalid rule ID `{id}`"),
            Self::InvalidRule(id, message) => write!(f, "invalid rule `{id}`: {message}"),
            Self::DuplicateRule(id) => write!(f, "duplicate rule `{id}`"),
        }
    }
}

impl Error for ProviderCatalogError {}

#[derive(Clone, Debug)]
/// Provider rules, namespaced IDs, and compiled plans.
pub struct RuleCatalog {
    /// Compiled rule records (no source declaration trees retained).
    records: Vec<CompiledRuleRecord>,
}

impl RuleCatalog {
    /// Build a provider catalog from locally named rules.
    pub fn new(
        provider: impl Into<String>,
        rules: Vec<Rule>,
    ) -> Result<Self, ProviderCatalogError> {
        let provider = provider.into();
        if !RuleId::valid_provider(&provider) {
            return Err(ProviderCatalogError::InvalidRuleId(provider));
        }

        let rules_and_ids = rules
            .into_iter()
            .map(|rule| {
                let rule_id = RuleId::from_provider_and_name(&provider, rule.id())?;
                Ok((rule_id, rule))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Compile once into immutable records (no declarations retained).
        let records = compile_records(&rules_and_ids).map_err(map_compiled_catalog_error)?;

        Ok(Self { records })
    }

    /// Combine validated provider catalogs under one shared host environment.
    ///
    /// Full namespaced rule IDs must remain unique. Local rule names may
    /// overlap between providers because catalog identity is retained by rule
    /// position rather than inferred from the local name.
    /// Combines catalogs by moving already-compiled records — never recompiles.
    /// Fully-qualified IDs are validated before any record is moved into the
    /// result, so a duplicate-ID error returns without a partially mutated
    /// destination.
    pub fn combine(catalogs: impl IntoIterator<Item = Self>) -> Result<Self, ProviderCatalogError> {
        let mut records = Vec::new();
        let mut seen = BTreeSet::new();

        // Validate all FQIDs before moving any record.
        for catalog in catalogs {
            for record in catalog.records {
                if !seen.insert(record.rule_id.clone()) {
                    return Err(ProviderCatalogError::DuplicateRule(record.rule_id));
                }
                records.push(record);
            }
        }

        Ok(Self { records })
    }

    #[must_use]
    /// Return report metadata in catalog order.
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.records
            .iter()
            .map(|record| {
                RuleMetadata::from_catalog(
                    record.rule_id.clone(),
                    record.description.clone(),
                    record.query_explanations.clone(),
                    record.severity,
                )
            })
            .collect()
    }

    /// Borrow fully-qualified rule IDs in catalog order.
    pub fn rule_ids(&self) -> impl Iterator<Item = &RuleId> {
        self.records.iter().map(|record| &record.rule_id)
    }

    /// Return the number of compiled rules in catalog order.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    /// Borrow the ID at a stable catalog index.
    pub(crate) fn rule_id(&self, index: RuleIndex) -> Option<&RuleId> {
        self.records.get(index.get()).map(|record| &record.rule_id)
    }

    /// Borrow compiled matcher plans.
    pub(crate) fn compiled(&self) -> &[CompiledRuleRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rule::{
        CompilerInvariantDiagnostic, Confidence, EventQuery, PhysicalPlanDiagnostic,
        QueryDiagnostic, Rule, Severity,
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

        assert_eq!(
            error,
            ProviderCatalogError::DuplicateRule(RuleId::parse("same:request").unwrap())
        );
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
                RuleCompilationError::InvalidPhysicalPlan(
                    "physical plan must contain a root".into(),
                ),
            ),
        ];

        for (compiled, expected) in cases {
            assert_eq!(
                map_compiled_catalog_error(compiled),
                ProviderCatalogError::InvalidRule(rule_id.clone(), expected)
            );
        }
    }
}
