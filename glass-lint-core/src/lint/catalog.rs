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
    /// A rule failed catalog validation, including duplicate identity.
    InvalidRule(RuleId, String),
}

impl fmt::Display for ProviderCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRuleId(id) => write!(f, "invalid rule ID `{id}`"),
            Self::InvalidRule(id, message) => write!(f, "invalid rule `{id}`: {message}"),
        }
    }
}

impl Error for ProviderCatalogError {}

#[derive(Clone, Debug)]
/// Provider rules, namespaced IDs, and compiled plans.
pub struct RuleCatalog {
    /// Compiled rule records (no source declaration trees retained).
    pub(crate) records: Vec<CompiledRuleRecord>,
    rule_ids: Vec<RuleId>,
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
                let validated = rule.validate_and_normalize().map_err(|error| {
                    ProviderCatalogError::InvalidRule(rule_id.clone(), error.to_string())
                })?;
                Ok((validated, rule_id))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let (rules, rule_ids): (Vec<_>, Vec<_>) = rules_and_ids.into_iter().unzip();

        // Compile once into immutable records (no declarations retained).
        let provider_compile =
            RuleId::parse(format!("{provider}:compile")).expect("valid provider and name");
        let records = compile_records(&rules).map_err(|error| match error {
            CompiledCatalogError::InvalidMatcher(message) => {
                ProviderCatalogError::InvalidRule(provider_compile, message)
            }
        })?;

        let rule_indices = rule_ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, RuleIndex::new(index)))
            .collect();
        Ok(Self {
            records,
            rule_ids,
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
        let mut rule_ids = Vec::new();
        let mut seen = BTreeSet::new();

        // Validate all FQIDs before moving any record.
        for catalog in catalogs {
            for (record, rule_id) in catalog.records.into_iter().zip(catalog.rule_ids) {
                if !seen.insert(rule_id.clone()) {
                    return Err(ProviderCatalogError::InvalidRule(
                        rule_id,
                        "duplicate rule".into(),
                    ));
                }
                // Stage the record and ID for insertion.
                records.push(record);
                rule_ids.push(rule_id);
            }
        }

        let rule_indices = rule_ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, RuleIndex::new(index)))
            .collect();
        Ok(Self {
            records,
            rule_ids,
            rule_indices,
        })
    }

    #[must_use]
    /// Return report metadata in catalog order.
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.records
            .iter()
            .zip(&self.rule_ids)
            .map(|(record, id)| RuleMetadata {
                id: id.clone(),
                description: record.description.clone(),
                default_severity: record.severity,
            })
            .collect()
    }

    #[must_use]
    /// Borrow fully-qualified rule IDs in catalog order.
    pub fn rule_ids(&self) -> &[RuleId] {
        &self.rule_ids
    }

    #[must_use]
    /// Borrow the ID at a stable catalog index.
    pub fn rule_id(&self, index: RuleIndex) -> Option<&RuleId> {
        self.rule_ids.get(index.get())
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
    use crate::api::rule::{Confidence, MatcherDecl, Rule, Severity};

    fn make_catalog(provider: &str) -> RuleCatalog {
        let rule = Rule::builder("request")
            .description("Request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .declaration(
                MatcherDecl::builder()
                    .call_global("fetch")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap();
        RuleCatalog::new(provider, vec![rule]).unwrap()
    }

    #[test]
    fn combined_catalog_rejects_duplicate_namespaced_ids() {
        let error = RuleCatalog::combine([make_catalog("same"), make_catalog("same")]).unwrap_err();

        assert_eq!(
            error,
            ProviderCatalogError::InvalidRule(
                RuleId::parse("same:request").unwrap(),
                "duplicate rule".into()
            )
        );
    }

    #[test]
    fn combined_catalog_moves_records_without_recompiling() {
        let combined = RuleCatalog::combine([make_catalog("a"), make_catalog("b")]).unwrap();
        assert_eq!(combined.rule_ids.len(), 2);
        assert_eq!(combined.records.len(), 2);
        assert_eq!(combined.rule_ids[0].as_str(), "a:request");
        assert_eq!(combined.rule_ids[1].as_str(), "b:request");
    }
}
