//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.

#![allow(clippy::redundant_pub_crate)]

pub(crate) mod catalog;
pub(crate) mod lowering;
pub(crate) mod object_flow;
pub(crate) mod rule;

pub(crate) use catalog::CompiledCatalog;
pub(crate) use object_flow::{
    CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments,
};
#[cfg(test)]
pub(crate) use rule::CompiledMatcherPlan;
pub(crate) use rule::{CompiledRule, CompiledRuleSelection};
