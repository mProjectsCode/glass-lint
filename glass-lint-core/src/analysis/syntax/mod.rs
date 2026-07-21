//! Foundational syntax identities and naming helpers.
//!
//! These helpers are intentionally syntax-directed and AST-independent after
//! evaluation. They normalize supported AST shapes (call chains, member
//! chains, binding patterns) and return `None` for dynamic or executable
//! forms, leaving semantic provenance and shadowing decisions to the scope
//! collector rather than duplicating them here.
//!
//! Provenance types (`SymbolCallProvenance`, `SymbolMemberProvenance`) live
//! here because they are produced by syntax-directed normalization and
//! consumed by both the scope collector and the fact builder.

pub(super) mod constant;
mod names;
mod provenance;

pub use names::*;
pub(in crate::analysis) use provenance::{
    BudgetComponent, SymbolCallProvenance, SymbolMemberProvenance, UnknownReason,
};
