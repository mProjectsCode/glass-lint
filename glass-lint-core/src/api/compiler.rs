mod catalog;
mod rule;

pub(crate) use catalog::CompiledCatalog;
pub(crate) use rule::{
    CompiledMatcherCatalog, CompiledMatcherPlan, CompiledObjectFlow, CompiledObjectRequirement,
    CompiledObjectSinkArgs, CompiledRule,
};

// TODO: why does this discard the compiled catalog? Also this is only used once and should be inlined into the caller
pub fn validate_catalog(
    rules: &[super::rule::ApiRule],
) -> Result<(), super::rule::ApiCatalogError> {
    CompiledCatalog::try_from_rules(rules).map(|_| ())
}
