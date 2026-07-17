//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.

pub mod catalog;
pub mod rule;

pub use catalog::CompiledCatalog;
#[cfg(test)]
pub use rule::CompiledMatcherPlan;
pub use rule::{
    CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments, CompiledRule,
    CompiledRuleSelection,
};
