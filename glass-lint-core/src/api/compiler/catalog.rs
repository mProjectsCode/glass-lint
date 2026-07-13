use std::collections::BTreeSet;

use crate::api::compiler::CompiledMatcherCatalog;

use super::super::rule::{ApiCatalogError, ApiRule};
use super::CompiledRule;

#[derive(Debug, Clone)]
pub(crate) struct CompiledCatalog {
    pub(crate) rules: Vec<CompiledRule>,
}

impl CompiledCatalog {
    pub(crate) fn try_from_rules(rules: &[ApiRule]) -> Result<Self, ApiCatalogError> {
        let mut ids = std::collections::BTreeSet::new();
        for rule in rules {
            if !ids.insert(rule.id().to_string()) {
                return Err(ApiCatalogError::DuplicateRule(rule.id().to_string()));
            }
        }
        Ok(Self {
            rules: rules.iter().map(CompiledRule::new).collect(),
        })
    }

    pub(crate) fn from_rules(rules: &[ApiRule]) -> Self {
        Self::try_from_rules(rules).expect("validated rule catalog")
    }

    pub(crate) fn to_matcher_catalog<'a>(
        &'a self,
        selected: &'a BTreeSet<usize>,
    ) -> CompiledMatcherCatalog<'a> {
        let matchers = self
            .rules
            .iter()
            .map(|rule| &rule.matcher)
            .collect::<Vec<_>>();
        CompiledMatcherCatalog::new(matchers, selected)
    }
}
