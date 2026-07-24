pub(super) mod eval;
#[cfg(test)]
mod tests;
mod types;

pub(in crate::analysis) use eval::{EvalState, Lookup, NoLookup, evaluate, property_name};
pub(in crate::analysis) use types::{ConstValue, non_negative_integer};
