mod catalog;
mod rule;

pub(crate) use catalog::CompiledCatalog;
pub(crate) use rule::{
    CompiledMatcherCatalog, CompiledMatcherPlan, CompiledObjectFlow, CompiledObjectRequirement,
    CompiledObjectSinkArgs, CompiledRule,
};

pub fn validate_catalog(
    rules: &[super::rule::ApiRule],
) -> Result<(), super::rule::ApiCatalogError> {
    CompiledCatalog::try_from_rules(rules).map(|_| ())
}
