//! Validated rule catalogs and stable rule-index selection.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    RuleId, RuleMetadata,
    api::{compiler::CompiledCatalog, rule::Rule},
};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Catalog construction failure.
pub enum ProviderCatalogError {
    /// Provider prefix or full rule ID is invalid.
    InvalidRuleId(String),
    /// A rule failed catalog validation, including duplicate identity.
    InvalidRule(String, String),
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
    /// Rules in stable declaration order.
    pub(crate) rules: Vec<Rule>,
    rule_ids: Vec<RuleId>,
    rule_indices: BTreeMap<RuleId, crate::api::classification::RuleIndex>,
    compiled: CompiledCatalog,
}

impl RuleCatalog {
    /// Build a provider catalog from locally named rules.
    pub fn new(
        provider: impl Into<String>,
        rules: Vec<Rule>,
    ) -> Result<Self, ProviderCatalogError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;

        let rules = rules
            .into_iter()
            .map(|rule| {
                let id = format!("{provider}:{}", rule.id());
                rule.validate_and_normalize()
                    .map_err(|error| ProviderCatalogError::InvalidRule(id, error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let compiled = CompiledCatalog::try_from_rules(&rules).map_err(|error| match error {
            crate::api::rule::CompiledCatalogError::DuplicateRule(id) => {
                ProviderCatalogError::InvalidRule(
                    format!("{provider}:{id}"),
                    "duplicate rule".into(),
                )
            }
            crate::api::rule::CompiledCatalogError::InvalidMatcher(message) => {
                ProviderCatalogError::InvalidRule("<catalog>".into(), message)
            }
        })?;

        let rule_ids = rules
            .iter()
            .map(|rule| RuleId::parse(format!("{provider}:{}", rule.id())))
            .collect::<Result<Vec<_>, _>>()?;

        let rule_indices = rule_ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, crate::api::classification::RuleIndex::new(index)))
            .collect();
        Ok(Self {
            rules,
            rule_ids,
            rule_indices,
            compiled,
        })
    }

    /// Combine validated provider catalogs under one shared host environment.
    ///
    /// Full namespaced rule IDs must remain unique. Local rule names may
    /// overlap between providers because catalog identity is retained by rule
    /// position rather than inferred from the local name.
    /// Combine catalogs while rejecting duplicate fully-qualified IDs.
    pub fn combine(catalogs: impl IntoIterator<Item = Self>) -> Result<Self, ProviderCatalogError> {
        let mut rules = Vec::new();
        let mut rule_ids = Vec::new();
        let mut seen = BTreeSet::new();

        for catalog in catalogs {
            for (rule, rule_id) in catalog.rules.into_iter().zip(catalog.rule_ids) {
                if !seen.insert(rule_id.clone()) {
                    return Err(ProviderCatalogError::InvalidRule(
                        rule_id.to_string(),
                        "duplicate rule".into(),
                    ));
                }
                rules.push(rule);
                rule_ids.push(rule_id);
            }
        }

        let rule_indices = rule_ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, crate::api::classification::RuleIndex::new(index)))
            .collect();
        let compiled = CompiledCatalog::compile_rules(&rules).map_err(|error| match error {
            crate::api::rule::CompiledCatalogError::DuplicateRule(id) => {
                ProviderCatalogError::InvalidRule(id, "duplicate rule".into())
            }
            crate::api::rule::CompiledCatalogError::InvalidMatcher(message) => {
                ProviderCatalogError::InvalidRule("<catalog>".into(), message)
            }
        })?;
        Ok(Self {
            rules,
            rule_ids,
            rule_indices,
            compiled,
        })
    }

    #[must_use]
    /// Return report metadata in catalog order.
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.rules
            .iter()
            .zip(&self.rule_ids)
            .map(|(rule, id)| RuleMetadata {
                id: id.clone(),
                description: rule.description().to_string(),
                default_severity: rule.severity(),
                messages: BTreeMap::from([(
                    String::from("detected"),
                    String::from("Detected matching capability"),
                )]),
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
    pub fn rule_id(&self, index: crate::api::classification::RuleIndex) -> Option<&RuleId> {
        self.rule_ids.get(index.get())
    }

    /// Borrow compiled matcher plans.
    pub(crate) fn compiled(&self) -> &CompiledCatalog {
        &self.compiled
    }

    /// Resolve a fully-qualified ID to its catalog index.
    pub fn rule_index(&self, id: &RuleId) -> Option<crate::api::classification::RuleIndex> {
        self.rule_indices.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CallMatcher, Confidence, Rule, Severity};

    fn catalog(provider: &str) -> RuleCatalog {
        let rule = Rule::builder("request")
            .description("Request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(CallMatcher::global("fetch"))
            .build()
            .unwrap();
        RuleCatalog::new(provider, vec![rule]).unwrap()
    }

    #[test]
    fn combined_catalog_rejects_duplicate_namespaced_ids() {
        let error = RuleCatalog::combine([catalog("same"), catalog("same")]).unwrap_err();

        assert_eq!(
            error,
            ProviderCatalogError::InvalidRule("same:request".into(), "duplicate rule".into())
        );
    }
}
