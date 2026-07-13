use super::{
    BindingProvenance, BoundArgument, IdentValueSeed, MemberValueSeed, ScopeGraph, ScopeKind,
};
use crate::analysis::scope::collect::aliases::{contains, member_prefix_ends};
use crate::analysis::syntax::constant::{self, ConstValue, EvalState, Lookup};
use crate::analysis::syntax::{
    self, SymbolCallProvenance, SymbolMemberProvenance, member_root_ident,
};
use crate::analysis::value::{BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath};
use swc_common::{Span, Spanned};
use swc_ecma_ast::{Expr, Ident, MemberExpr};

mod bindings;
mod constants;
mod functions;
mod provenance;
pub(in crate::analysis) mod rooted;
