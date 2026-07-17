//! Rule catalog construction and duplicate-ID validation.

use super::{
    super::rule::{CatalogError, Rule},
    CompiledRule,
};
use crate::api::compiler::CompiledMatcherCatalog;

#[derive(Debug, Clone)]
/// Compiled rules in deterministic declaration order.
pub struct CompiledCatalog {
    /// Immutable compiled matcher plans indexed by rule order.
    pub rules: Vec<CompiledRule>,
}

impl CompiledCatalog {
    /// Compile rules after rejecting duplicate stable IDs.
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

    /// Compile rules without duplicate validation for trusted callers.
    pub fn from_rules(rules: &[Rule]) -> Self {
        Self {
            rules: rules.iter().map(CompiledRule::new).collect(),
        }
    }

    /// Borrow a selected-rule view over this catalog.
    pub fn to_matcher_catalog<'a>(
        &'a self,
        selected: &'a [crate::api::classification::RuleIndex],
    ) -> CompiledMatcherCatalog<'a> {
        CompiledMatcherCatalog::new(&self.rules, selected)
    }
}
