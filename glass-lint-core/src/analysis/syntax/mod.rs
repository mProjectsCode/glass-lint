//! Foundational syntax identities and naming helpers.

pub(super) mod constant;
mod names;
mod provenance;

pub use names::*;
pub(in crate::analysis) use provenance::{SymbolCallProvenance, SymbolMemberProvenance};
