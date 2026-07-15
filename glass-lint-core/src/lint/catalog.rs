use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    Environment, RuleId, RuleMetadata,
    api::{
        compiler::{CompiledCatalog, validate_catalog},
        rule::ApiRule,
    },
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
    pub(crate) rules: Vec<ApiRule>,
    rule_ids: Vec<RuleId>,
    environment: Environment,
}

impl RuleCatalog {
    pub fn new(provider: impl Into<String>, rules: Vec<ApiRule>) -> Result<Self, RuleCatalogError> {
        Self::with_environment(provider, rules, Environment::default())
    }

    pub fn with_environment(
        provider: impl Into<String>,
        rules: Vec<ApiRule>,
        environment: Environment,
    ) -> Result<Self, RuleCatalogError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;

        // TODO: inline this call and think if this really needs to compile and throw away the compiled catalog. We could probably just validate the rules in place.
        validate_catalog(&rules).map_err(|error| match error {
            crate::api::rule::ApiCatalogError::DuplicateRule(id) => {
                RuleCatalogError::InvalidRule(format!("{provider}:{id}"), "duplicate rule".into())
            }
        })?;

        let rule_ids = rules
            .iter()
            .map(|rule| RuleId::parse(format!("{provider}:{}", rule.id())))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            rules,
            rule_ids,
            environment,
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

        Ok(Self {
            rules,
            rule_ids,
            environment,
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

    pub(crate) fn rule_id(&self, index: usize) -> Option<&RuleId> {
        self.rule_ids.get(index)
    }

    pub(crate) fn compiled(&self) -> CompiledCatalog {
        CompiledCatalog::from_rules(&self.rules)
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
