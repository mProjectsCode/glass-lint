//! Source-order collection of conservative lexical and alias facts.
//!
//! [`ScopePlanner`](plan::ScopePlanner) establishes declaration visibility and
//! structural scope identities. [`ScopeCollector`] then traverses the source
//! in order to collect references, assignments, and mutation.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use hashbrown::{HashMap, HashSet};

use glass_lint_datastructures::{NameId, NameTable};
use history::AssignmentHistory;
use smol_str::SmolStr;
use swc_common::BytePos;

use crate::analysis::{
    SemanticBudget,
    scope::{
        AliasAssignment, BindingProvenance, LexicalScope, ScopeEffect, ScopeId, ScopeKind,
        ScopedName,
    },
};

pub(super) mod aliases;
mod analysis;
mod assignments;
mod bindings;
mod callbacks;
mod collector;
pub(super) mod compact_pat;
mod constants;
mod freeze;
mod history;
pub(super) mod plan;
pub(super) mod program;
mod projection;
mod provenance;
pub(super) mod shape;
pub(super) mod traversal;
pub(super) mod visitor;

pub(super) use compact_pat::{CompactPat, compact_pat};
pub(in crate::analysis) use program::{ScopedProgram, ScopeCollectionIssue};
pub(super) use program::{PropertyAliasAssignment, RootedPropertyMutation};
pub(super) use shape::{ScopeShape, ScopeShapeTable};

/// Mutable state shared by declaration prepass and source-order collection.
///
/// The prepass establishes lexical binding identity; the normal visitor then
/// reuses that scope tree while recording assignments and supported
/// provenance at each use position.
pub(super) struct ScopeCollector<'a> {
    /// Lexical scopes in predeclaration/traversal order.
    pub(super) scopes: Vec<LexicalScope>,
    /// Current lexical path during AST traversal.
    stack: Vec<usize>,
    /// Assignment events retain source order for use-position provenance.
    pub(super) assignments: Vec<AliasAssignment>,
    /// Latest use-position assignment state per lexical scope.
    latest_assignments: AssignmentHistory,
    /// Property writes retained for flow-aware rooted-member queries.
    pub(super) property_assignments: Vec<PropertyAliasAssignment>,
    /// Writes that invalidate a rooted receiver/property identity.
    pub(super) rooted_property_mutations: Vec<RootedPropertyMutation>,
    /// Dynamic `eval` sites that make local provenance conservative.
    pub(super) dynamic_evals: Vec<(ScopeId, ScopeEffect)>,
    /// Function scopes and their parameter patterns by visible NameId.
    pub(super) function_scopes: HashMap<(ScopeId, NameId), (ScopeId, Vec<CompactPat>)>,
    /// Aliases that point to a locally declared helper function.
    pub(super) function_aliases: HashMap<ScopedName, ScopeId>,
    /// Calls retained for the later, scope-aware helper parameter pass.
    calls: Vec<(ScopeId, NameId, Vec<Option<BindingProvenance>>)>,
    /// Proven callback arguments installed when an inline function is entered.
    inline_parameters: HashMap<BytePos, HashMap<SmolStr, BindingProvenance>>,
    /// `var`-bound objects whose mutation prevents constant projection.
    pub(super) mutable_static_objects: HashSet<ScopedName>,
    /// Function expression names stashed by `visit_var_decl` and consumed
    /// by `after_function` / `after_arrow` hooks so `function_scopes` is
    /// recorded only for var/let/const declared function expressions.
    pending_function_names: HashMap<BytePos, (ScopeId, NameId)>,
    names: NameTable,
    pub(super) name_exhausted: bool,
    /// Per (scope, name) counter to avoid rescanniing all assignments.
    version_counters: HashMap<(ScopeId, NameId), u32>,
    /// Structural scope shape table produced by the planner and consumed by
    /// the source-order visitor.
    scope_shapes: ScopeShapeTable,
    /// A phase mismatch is a conservative incomplete analysis, not a panic.
    scope_issues: Vec<ScopeCollectionIssue>,
    /// Shared semantic budget charged for each name interning operation.
    budget: &'a SemanticBudget,
    #[cfg(test)]
    scope_lookups: usize,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
