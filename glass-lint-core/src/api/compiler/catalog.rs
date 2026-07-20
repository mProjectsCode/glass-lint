//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{
        compiler::{CompiledRule, CompiledRuleSelection},
        rule::CompiledCatalogError,
    },
};

#[derive(Debug, Clone)]
/// Compiled rules in deterministic declaration order.
pub(crate) struct CompiledCatalog {
    /// Immutable compiled matcher plans indexed by rule order.
    pub(crate) rules: Vec<CompiledRule>,
}

impl CompiledCatalog {
    /// Compile rules after rejecting duplicate stable IDs.
    pub fn try_from_rules(rules: &[Rule]) -> Result<Self, CompiledCatalogError> {
        let mut ids = std::collections::BTreeSet::new();
        for rule in rules {
            if !ids.insert(rule.id().to_string()) {
                return Err(CompiledCatalogError::DuplicateRule(rule.id().to_string()));
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
    ) -> CompiledRuleSelection<'a> {
        CompiledRuleSelection::new(&self.rules, selected)
    }
}
