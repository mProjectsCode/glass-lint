//! Validated rule catalogs and stable rule-index selection.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

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
    InvalidRule(RuleId, String),
    /// A fully-qualified rule ID occurs in more than one catalog.
    DuplicateRule(RuleId),
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
    rule_indices: BTreeMap<RuleId, RuleIndex>,
}

impl RuleCatalog {
    /// Build a provider catalog from locally named rules.
    pub fn new(
        provider: impl Into<String>,
        rules: Vec<Rule>,
    ) -> Result<Self, ProviderCatalogError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;

        let rules_and_ids = rules
            .into_iter()
            .map(|rule| {
                let rule_id = RuleId::parse(format!("{provider}:{}", rule.id()))?;
                let validated = rule.require_queries().map_err(|error| {
                    ProviderCatalogError::InvalidRule(rule_id.clone(), error.to_string())
                })?;
                Ok((rule_id, validated))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Compile once into immutable records (no declarations retained).
        let records = compile_records(&rules_and_ids).map_err(|error| match error {
            CompiledCatalogError::InvalidMatcher { rule_id, message }
            | CompiledCatalogError::CompilerInvariant { rule_id, message }
            | CompiledCatalogError::InvalidPhysicalPlan { rule_id, message } => {
                ProviderCatalogError::InvalidRule(
                    RuleId::parse(rule_id).expect("compiler preserves validated rule ID"),
                    message,
                )
            }
            CompiledCatalogError::InvalidQuery {
                rule_id,
                diagnostic,
            } => ProviderCatalogError::InvalidRule(
                RuleId::parse(rule_id).expect("compiler preserves validated rule ID"),
                diagnostic.to_string(),
            ),
        })?;

        let rule_indices = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.rule_id.clone(), RuleIndex::new(index)))
            .collect();
        Ok(Self {
            records,
            rule_indices,
        })
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

        let rule_indices = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.rule_id.clone(), RuleIndex::new(index)))
            .collect();
        Ok(Self {
            records,
            rule_indices,
        })
    }

    #[must_use]
    /// Return report metadata in catalog order.
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.records
            .iter()
            .map(|record| RuleMetadata {
                id: record.rule_id.clone(),
                description: record.description.clone(),
                query_explanations: record.query_explanations.clone(),
                default_severity: record.severity,
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
    pub fn rule_id(&self, index: RuleIndex) -> Option<&RuleId> {
        self.records.get(index.get()).map(|record| &record.rule_id)
    }

    /// Borrow compiled matcher plans.
    pub(crate) fn compiled(&self) -> &[CompiledRuleRecord] {
        &self.records
    }

    /// Resolve a fully-qualified ID to its catalog index.
    pub fn rule_index(&self, id: &RuleId) -> Option<RuleIndex> {
        self.rule_indices.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rule::{Category, Confidence, EventQuery, Rule, Severity};

    fn make_catalog(provider: &str) -> RuleCatalog {
        let rule = Rule::builder("request")
            .description("Request")
            .category(Category::new("network").unwrap())
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
}
