//! Source-order collection of conservative lexical and alias facts.
//!
//! [`ScopePlanner`](plan::ScopePlanner) establishes declaration visibility and
//! structural scope identities. [`ScopeCollector`] then traverses the source
//! in order to collect references, assignments, and mutation.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use glass_lint_datastructures::{NameId, NameTable};
use hashbrown::{HashMap, HashSet};
use history::{AssignmentEnvironment, Cursor};
use smol_str::SmolStr;
use swc_common::BytePos;

use crate::analysis::{
    SemanticBudget,
    scope::{AliasAssignment, BindingProvenance, LexicalScope, ScopeEffect, ScopeId, ScopedName},
};

pub(super) mod aliases;
mod analysis;
mod assignments;
mod bindings;
mod callbacks;
mod collector;
mod compact_pat;
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
pub(super) use program::{PropertyAliasAssignment, RootedPropertyMutation};
pub(in crate::analysis) use program::{ScopeCollectionIssue, ScopedProgram};
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
    /// Nesting depth of conditional branches (if/else, loops, switch cases).
    /// An assignment is conditional when depth > 0.
    conditional_depth: u32,
    /// Path-sensitive assignment state for source-order provenance.
    assignment_environment: AssignmentEnvironment,
    /// Writes made since the current control-flow checkpoint.
    assignment_writes: std::collections::BTreeSet<ScopedName>,
    /// An explicit local value used when a path join disagrees.
    unknown_provenance: BindingProvenance,
    /// Active control-flow joins owned by the collector.
    control_flow: Vec<ControlFlowFrame>,
    /// Function-body checkpoints prevent local control flow from escaping the
    /// function declaration into source-order collection of its parent.
    function_checkpoints: Vec<(CollectorCheckpoint, u32, usize)>,
    reachable: bool,
    alternative_limit: usize,
    #[cfg(test)]
    scope_lookups: usize,
}

/// A cursor into the assignment environment's mutation log, with write-set
/// and reachability for restore or join.
#[derive(Debug, Clone)]
struct CollectorCheckpoint {
    cursor: Cursor,
    writes: std::collections::BTreeSet<ScopedName>,
    reachable: bool,
}

#[derive(Debug)]
enum ControlFlowFrame {
    If {
        incoming: CollectorCheckpoint,
        consequent: Option<CollectorCheckpoint>,
    },
    Loop {
        incoming: CollectorCheckpoint,
        guaranteed: bool,
        breaks: Vec<CollectorCheckpoint>,
        continues: Vec<CollectorCheckpoint>,
    },
    Switch {
        incoming: CollectorCheckpoint,
        cases: Vec<CollectorCheckpoint>,
        breaks: Vec<CollectorCheckpoint>,
    },
    Try {
        incoming: CollectorCheckpoint,
        body: Option<CollectorCheckpoint>,
        conditional: bool,
    },
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
