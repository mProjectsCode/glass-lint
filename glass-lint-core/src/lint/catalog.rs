use crate::api::{
    compiler::{CompiledCatalog, validate_catalog},
    rule::{ApiRule, ApiSeverity},
};
use crate::{RuleId, RuleMetadata, Severity};
use std::collections::BTreeMap;
use std::{error::Error, fmt};

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
    namespaced: BTreeMap<String, RuleId>,
}

impl RuleCatalog {
    pub fn new(provider: impl Into<String>, rules: Vec<ApiRule>) -> Result<Self, RuleCatalogError> {
        let provider = provider.into();
        RuleId::parse(format!("{provider}:placeholder"))?;
        validate_catalog(&rules).map_err(|error| match error {
            crate::api::rule::ApiCatalogError::DuplicateRule(id) => {
                RuleCatalogError::InvalidRule(format!("{provider}:{id}"), "duplicate rule".into())
            }
        })?;
        let mut namespaced = BTreeMap::new();
        for rule in &rules {
            namespaced.insert(
                rule.id().to_string(),
                RuleId::parse(format!("{provider}:{}", rule.id()))?,
            );
        }
        Ok(Self { rules, namespaced })
    }
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.rules
            .iter()
            .filter_map(|rule| {
                self.namespaced_id(rule.id()).map(|id| RuleMetadata {
                    id: id.clone(),
                    description: rule.label().to_string(),
                    default_severity: severity(rule.severity()),
                    messages: BTreeMap::from([(
                        String::from("detected"),
                        String::from("Detected matching capability"),
                    )]),
                })
            })
            .collect()
    }
    pub fn rule_ids(&self) -> Vec<RuleId> {
        self.rules
            .iter()
            .filter_map(|rule| self.namespaced_id(rule.id()).cloned())
            .collect()
    }
    pub(crate) fn namespaced_id(&self, id: &str) -> Option<&RuleId> {
        self.namespaced.get(id)
    }
    pub(crate) fn compiled(&self) -> CompiledCatalog {
        CompiledCatalog::from_rules(&self.rules)
    }
}

fn severity(value: ApiSeverity) -> Severity {
    match value {
        ApiSeverity::Info => Severity::Info,
        ApiSeverity::Warning => Severity::Warning,
        ApiSeverity::Error => Severity::Error,
    }
}
