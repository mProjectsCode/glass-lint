//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{
        compiler::{CompiledRule, CompiledRuleRecord, CompiledRuleSelection},
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
    /// Compile rules into record form (metadata + plan, no retained decls).
    pub fn compile_records(
        rules: &[Rule],
    ) -> Result<Vec<CompiledRuleRecord>, CompiledCatalogError> {
        rules
            .iter()
            .map(|rule| {
                CompiledRuleRecord::new(rule)
                    .map_err(|e| CompiledCatalogError::InvalidMatcher(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Borrow a selected-rule view over this catalog.
    pub fn to_matcher_catalog<'a>(
        &'a self,
        selected: &'a [crate::api::classification::RuleIndex],
    ) -> CompiledRuleSelection<'a> {
        CompiledRuleSelection::new(&self.rules, selected)
    }
}
