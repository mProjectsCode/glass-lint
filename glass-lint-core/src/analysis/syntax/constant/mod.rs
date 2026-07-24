mod types;
pub(super) mod eval;
#[cfg(test)]
mod tests;

pub(in crate::analysis) use types::ConstValue;
pub(in crate::analysis) use types::non_negative_integer;
pub(in crate::analysis) use eval::{EvalState, Lookup, NoLookup, evaluate, property_name};
