use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    Environment, RuleId, RuleMetadata,
    api::{compiler::CompiledCatalog, rule::Rule},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleCatalogError {
    InvalidRuleId(String),
    InvalidRule(String, String),
}

impl fmt::Display for RuleCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRuleId(id) => write!(f, "invalid rule ID `{id}`"),
            Self::InvalidRule(id, message) => write!(f, "invalid rule `{id}`: {message}"),
        }
    }
}

impl Error for RuleCatalogError {}

#[derive(Clone, Debug)]
pub struct RuleCatalog {
    pub rules: Vec<Rule>,
    rule_ids: Vec<RuleId>,
    rule_indices: BTreeMap<RuleId, usize>,
    environment: Environment,
    compiled: CompiledCatalog,
}

impl RuleCatalog {
    pub fn new(provider: impl Into<String>, rules: Vec<Rule>) -> Result<Self, RuleCatalogError> {
        Self::with_environment(provider, rules, Environment::default())
    }

    pub fn with_environment(
        provider: impl Into<String>,
        rules: Vec<Rule>,
        environment: Environment,
    ) -> Result<Self, RuleCatalogError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;

        let compiled = CompiledCatalog::try_from_rules(&rules).map_err(|error| match error {
            crate::api::rule::CatalogError::DuplicateRule(id) => {
                RuleCatalogError::InvalidRule(format!("{provider}:{id}"), "duplicate rule".into())
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
            .map(|(index, id)| (id, index))
            .collect();
        Ok(Self {
            rules,
            rule_ids,
            rule_indices,
            environment,
            compiled,
        })
    }

    /// Combine validated provider catalogs under one shared host environment.
    ///
    /// Full namespaced rule IDs must remain unique. Local rule names may
    /// overlap between providers because catalog identity is retained by rule
    /// position rather than inferred from the local name.
    pub fn combine_with_environment(
        catalogs: impl IntoIterator<Item = Self>,
        environment: Environment,
    ) -> Result<Self, RuleCatalogError> {
        let mut rules = Vec::new();
        let mut rule_ids = Vec::new();
        let mut seen = BTreeSet::new();

        for catalog in catalogs {
            for (rule, rule_id) in catalog.rules.into_iter().zip(catalog.rule_ids) {
                if !seen.insert(rule_id.clone()) {
                    return Err(RuleCatalogError::InvalidRule(
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
            .map(|(index, id)| (id, index))
            .collect();
        let compiled = CompiledCatalog::from_rules(&rules);
        Ok(Self {
            rules,
            rule_ids,
            rule_indices,
            environment,
            compiled,
        })
    }

    #[must_use]
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.rules
            .iter()
            .zip(&self.rule_ids)
            .map(|(rule, id)| RuleMetadata {
                id: id.clone(),
                description: rule.label().to_string(),
                default_severity: rule.severity().as_diagnostic_severity(),
                messages: BTreeMap::from([(
                    String::from("detected"),
                    String::from("Detected matching capability"),
                )]),
            })
            .collect()
    }

    #[must_use]
    pub fn rule_ids(&self) -> Vec<RuleId> {
        self.rule_ids.clone()
    }

    #[must_use]
    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn rule_id(&self, index: usize) -> Option<&RuleId> {
        self.rule_ids.get(index)
    }

    pub fn compiled(&self) -> &CompiledCatalog {
        &self.compiled
    }

    pub fn rule_index(&self, id: &RuleId) -> Option<usize> {
        self.rule_indices.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{CallMatcher, Confidence, Rule, Severity};

    fn catalog(provider: &str) -> RuleCatalog {
        let rule = Rule::builder("request")
            .label("Request")
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
        let error = RuleCatalog::combine_with_environment(
            [catalog("same"), catalog("same")],
            Environment::default(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            RuleCatalogError::InvalidRule("same:request".into(), "duplicate rule".into())
        );
    }
}
