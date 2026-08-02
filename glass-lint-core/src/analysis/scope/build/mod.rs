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
use history::{
    AssignmentEnvironment, Cursor, DEFAULT_ALTERNATIVE_LIMIT, WriteCheckpoint, WriteSet,
};
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

/// Collected outputs that are finalized into the immutable scope artifact.
#[derive(Default)]
pub(super) struct ScopeCollectionArtifacts {
    pub(super) property_assignments: Vec<PropertyAliasAssignment>,
    pub(super) rooted_property_mutations: Vec<RootedPropertyMutation>,
    pub(super) dynamic_evals: Vec<ScopedDynamicEval>,
    pub(super) mutable_static_objects: HashSet<ScopedName>,
    pub(super) scope_issues: Vec<ScopeCollectionIssue>,
}

/// A dynamic evaluation retained with the scope in which it was observed.
#[derive(Debug)]
pub(in crate::analysis) struct ScopedDynamicEval {
    pub(in crate::analysis) scope: ScopeId,
    pub(in crate::analysis) effect: ScopeEffect,
}

pub(super) struct FunctionBinding {
    scope: ScopeId,
    parameters: Vec<CompactPat>,
}

struct FunctionCall {
    caller_scope: ScopeId,
    callee_name: NameId,
    arguments: Vec<Option<BindingProvenance>>,
}

struct PendingFunctionName {
    declaration_scope: ScopeId,
    name: NameId,
}

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
    /// Collected outputs and conservative collection diagnostics.
    pub(super) artifacts: ScopeCollectionArtifacts,
    /// Function scopes and their parameter patterns by visible NameId.
    pub(super) function_scopes: HashMap<ScopedName, FunctionBinding>,
    /// Aliases that point to a locally declared helper function.
    pub(super) function_aliases: HashMap<ScopedName, ScopeId>,
    /// Calls retained for the later, scope-aware helper parameter pass.
    calls: Vec<FunctionCall>,
    /// Proven callback arguments installed when an inline function is entered.
    inline_parameters: HashMap<BytePos, HashMap<SmolStr, BindingProvenance>>,
    /// Function expression names stashed by `visit_var_decl` and consumed
    /// by `after_function` / `after_arrow` hooks so `function_scopes` is
    /// recorded only for var/let/const declared function expressions.
    pending_function_names: HashMap<BytePos, PendingFunctionName>,
    names: NameTable,
    pub(super) name_exhausted: bool,
    /// Per (scope, name) counter to avoid rescanniing all assignments.
    version_counters: HashMap<ScopedName, u32>,
    /// Structural scope shape table produced by the planner and consumed by
    /// the source-order visitor.
    scope_shapes: ScopeShapeTable,
    /// Shared semantic budget charged for each name interning operation.
    budget: &'a SemanticBudget,
    /// Path-sensitive assignment and control-flow state owned by collection.
    path_state: PathCollectionState,
    #[cfg(test)]
    scope_lookups: usize,
}

/// State for source-order assignment provenance and control-flow joins.
#[derive(Debug)]
struct PathCollectionState {
    conditional_depth: u32,
    assignment_environment: AssignmentEnvironment,
    assignment_writes: WriteSet,
    unknown_provenance: BindingProvenance,
    control_flow: Vec<ControlFlowFrame>,
    function_checkpoints: Vec<FunctionCheckpoint>,
    reachable: bool,
    alternative_limit: usize,
}

impl Default for PathCollectionState {
    fn default() -> Self {
        Self {
            conditional_depth: 0,
            assignment_environment: AssignmentEnvironment::new(),
            assignment_writes: WriteSet::new(),
            unknown_provenance: BindingProvenance::Local,
            control_flow: Vec::new(),
            function_checkpoints: Vec::new(),
            reachable: true,
            alternative_limit: DEFAULT_ALTERNATIVE_LIMIT,
        }
    }
}

/// A cursor into the assignment environment's mutation log, with write-set
/// and reachability for restore or join.
#[derive(Debug, Clone)]
struct CollectorCheckpoint {
    cursor: Cursor,
    writes: WriteCheckpoint,
    reachable: bool,
}

/// Assignment and control-flow state to restore when leaving a function.
#[derive(Debug)]
struct FunctionCheckpoint {
    checkpoint: CollectorCheckpoint,
    conditional_depth: u32,
    control_depth: usize,
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
