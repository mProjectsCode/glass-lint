use swc_common::{Span, Spanned};
use swc_ecma_ast::{Expr, Ident, MemberExpr};

use super::{
    BindingProvenance, BoundArgument, IdentValueSeed, MemberValueSeed, ScopeGraph, ScopeKind,
};
use crate::analysis::{
    scope::collect::aliases::{contains, member_prefix_ends},
    syntax::{
        self, SymbolCallProvenance, SymbolMemberProvenance,
        constant::{self, ConstValue, EvalState, Lookup},
        member_root_ident,
    },
    value::{BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath},
};

mod bindings;
mod constants;
mod functions;
mod provenance;
pub(in crate::analysis) mod rooted;
