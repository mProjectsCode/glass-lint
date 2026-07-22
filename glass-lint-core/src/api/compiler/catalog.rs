//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{compiler::CompiledRuleRecord, rule::CompiledCatalogError},
};

/// Compile rules into records in deterministic declaration order.
pub(crate) fn compile_records(
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
