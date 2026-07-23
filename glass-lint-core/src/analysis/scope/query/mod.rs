//! Position-sensitive queries over the immutable scope graph.
//!
//! Query modules expose small adapters for binding identity, constants,
//! function targets, module provenance, and rooted chains. They fail closed
//! when dynamic lookup, shadowing, or mutation makes a fact ambiguous.

use swc_common::Span;
use swc_ecma_ast::{Expr, Ident, MemberExpr};

use crate::analysis::{
    scope::{
        BindingProvenance, BoundArgument, FrozenScopeGraph, IdentValueSeed, MemberValueSeed,
        ScopeId, ScopeKind, collect::aliases::contains,
    },
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance,
        constant::{self, ConstValue, EvalState, Lookup},
        member_root_identifier,
    },
    value::{BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath},
};

mod bindings;
mod constants;
mod functions;
mod provenance;
pub(in crate::analysis) mod rooted;
