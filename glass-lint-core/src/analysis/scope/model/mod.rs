//! Structural scope graph types and collected alias facts.
//!
//! IDs are assigned after collection and are stable within one analyzed
//! module. Assignment versions and source spans remain part of the query
//! contract so aliases cannot cross a reassignment or lexical boundary.

pub(super) mod binding_index;
pub(super) mod frozen_assignments;
pub(super) mod graph;
pub(super) mod id;
pub(super) mod mutation_index;
pub(super) mod name_env;
pub(super) mod scope_index;
mod types;

pub(in crate::analysis) use frozen_assignments::FrozenAssignmentIndex;
pub(in crate::analysis) use graph::{FrozenScopeGraph, ScopeGraph, ScopeGraphParts};
pub(in crate::analysis) use id::{ScopeId, ScopedName};
pub(in crate::analysis) use types::{
    AliasAssignment, BindingProvenance, BoundArgument, IdentValueSeed, LexicalScope,
    MemberValueSeed, ScopeEffect, ScopeKind,
};
