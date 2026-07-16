use super::{
    super::rule::{CatalogError, Rule},
    CompiledRule,
};
use crate::api::compiler::CompiledMatcherCatalog;

#[derive(Debug, Clone)]
pub struct CompiledCatalog {
    pub rules: Vec<CompiledRule>,
}

impl CompiledCatalog {
    pub fn try_from_rules(rules: &[Rule]) -> Result<Self, CatalogError> {
        let mut ids = std::collections::BTreeSet::new();
        for rule in rules {
            if !ids.insert(rule.id().to_string()) {
                return Err(CatalogError::DuplicateRule(rule.id().to_string()));
            }
        }
        Ok(Self {
            rules: rules.iter().map(CompiledRule::new).collect(),
        })
    }

    pub fn from_rules(rules: &[Rule]) -> Self {
        Self {
            rules: rules.iter().map(CompiledRule::new).collect(),
        }
    }

    pub fn to_matcher_catalog<'a>(&'a self, selected: &'a [usize]) -> CompiledMatcherCatalog<'a> {
        CompiledMatcherCatalog::new(&self.rules, selected)
    }
}
