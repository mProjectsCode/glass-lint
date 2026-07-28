//! Immutable matcher compilation and catalog selection.
//!
//! Compilation translates validated public matcher declarations once. The
//! resulting plans are provider-neutral and can be projected onto many files
//! without rebuilding matcher semantics.

#![allow(clippy::redundant_pub_crate)]

pub(crate) mod catalog;
pub(crate) mod normalize;
pub(crate) mod object_flow;
pub(crate) mod rule;
pub(crate) mod validate;

pub(crate) use catalog::compile_records;
pub(crate) use object_flow::{
    CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments,
};
pub(crate) use rule::{CompiledRuleRecord, CompiledRuleSelection};
