//! Foundational syntax identities and naming helpers.
//!
//! These helpers are intentionally syntax-directed. They normalize supported
//! AST shapes and return `None` for dynamic or executable forms, leaving
//! semantic provenance and shadowing decisions to the scope collector.

pub(super) mod constant;
mod names;
mod provenance;

pub use names::*;
pub(in crate::analysis) use provenance::{
    BudgetComponent, SymbolCallProvenance, SymbolMemberProvenance, UnknownReason,
};
