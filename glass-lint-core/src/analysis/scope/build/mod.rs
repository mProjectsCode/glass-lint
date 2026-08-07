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
    scope::{AliasAssignment, BindingProvenance, LexicalScopes, ScopeEffect, ScopeId, ScopedName},
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
pub(super) use program::{PropertyAliasAssignment, RootedPropertyMutation, ScopeCollectionIssue};
pub(super) use shape::{ScopeShape, ScopeShapeTable};

/// Collected outputs that are finalized into the immutable scope artifact.
#[derive(Default)]
pub(super) struct ScopeCollectionArtifacts {
    property_assignments: Vec<PropertyAliasAssignment>,
    rooted_property_mutations: Vec<RootedPropertyMutation>,
    dynamic_evals: Vec<ScopedDynamicEval>,
    mutable_static_objects: HashSet<ScopedName>,
    scope_issues: Vec<ScopeCollectionIssue>,
}

impl ScopeCollectionArtifacts {
    pub(super) fn record_property_assignment(&mut self, assignment: PropertyAliasAssignment) {
        self.property_assignments.push(assignment);
    }

    pub(super) fn record_rooted_property_mutation(&mut self, mutation: RootedPropertyMutation) {
        self.rooted_property_mutations.push(mutation);
    }

    pub(super) fn record_dynamic_eval(&mut self, eval: ScopedDynamicEval) {
        self.dynamic_evals.push(eval);
    }

    pub(super) fn record_mutable_static_object(&mut self, name: ScopedName) {
        self.mutable_static_objects.insert(name);
    }

    pub(super) fn has_mutable_static_object(&self, name: &ScopedName) -> bool {
        self.mutable_static_objects.contains(name)
    }

    pub(super) fn record_issue(&mut self, issue: ScopeCollectionIssue) {
        self.scope_issues.push(issue);
    }

    pub(super) fn has_issues(&self) -> bool {
        !self.scope_issues.is_empty()
    }

    /// Consume collection records into the one bundle accepted by freezing.
    pub(super) fn seal(self) -> FrozenScopeCollectionArtifacts {
        FrozenScopeCollectionArtifacts {
            property_assignments: FrozenPropertyArtifacts {
                property_assignments: self.property_assignments,
                rooted_property_mutations: self.rooted_property_mutations,
                dynamic_evals: self.dynamic_evals,
            },
            mutable_static_objects: self.mutable_static_objects,
            scope_issues: self.scope_issues,
        }
    }
}

pub(super) struct FrozenScopeCollectionArtifacts {
    property_assignments: FrozenPropertyArtifacts,
    mutable_static_objects: HashSet<ScopedName>,
    scope_issues: Vec<ScopeCollectionIssue>,
}

pub(in crate::analysis) struct FrozenPropertyArtifacts {
    pub(in crate::analysis) property_assignments: Vec<PropertyAliasAssignment>,
    pub(in crate::analysis) rooted_property_mutations: Vec<RootedPropertyMutation>,
    pub(in crate::analysis) dynamic_evals: Vec<ScopedDynamicEval>,
}

/// A dynamic evaluation retained with the scope in which it was observed.
#[derive(Debug)]
pub(in crate::analysis) struct ScopedDynamicEval {
    scope: ScopeId,
    effect: ScopeEffect,
}

impl ScopedDynamicEval {
    pub(super) fn new(scope: ScopeId, effect: ScopeEffect) -> Self {
        Self { scope, effect }
    }

    pub(in crate::analysis) fn into_parts(self) -> (ScopeId, ScopeEffect) {
        (self.scope, self.effect)
    }
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
    pub(super) scopes: LexicalScopes,
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
