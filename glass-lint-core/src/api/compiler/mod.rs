//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.

#![allow(clippy::redundant_pub_crate)]

pub(crate) mod catalog;
pub(crate) mod rule;

pub(crate) use catalog::CompiledCatalog;
#[cfg(test)]
pub(crate) use rule::CompiledMatcherPlan;
pub(crate) use rule::{
    CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments, CompiledRule,
    CompiledRuleSelection,
};
