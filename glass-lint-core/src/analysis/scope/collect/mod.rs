//! Source-order collection of conservative lexical and alias facts.
//!
//! [`ScopePlanner`](plan::ScopePlanner) establishes declaration visibility and
//! structural scope identities. [`ScopeCollector`] then traverses the source
//! in order to collect references, assignments, and mutation.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use glass_lint_datastructures::{NameId, NamePath, NameTable, SymbolPath};
use history::AssignmentHistory;
use smol_str::{SmolStr, ToSmolStr};
use swc_common::{BytePos, Span};
use swc_ecma_ast::{ArrowExpr, Expr, Function, ImportDecl, ObjectPatProp, Pat, VarDeclKind};

use crate::{
    Environment,
    analysis::{
        SemanticBudget,
        scope::{
            AliasAssignment, BindingProvenance, FrozenAssignmentIndex, FrozenScopeGraph,
            LexicalScope, ScopeEffect, ScopeGraph, ScopeGraphParts, ScopeId, ScopeKind, ScopedName,
            query::rooted::{RootedExprContext, rooted_expr_chain_with},
        },
        syntax::{
            function_prototype_builtin, is_function_constructor_member, member_property_name,
            member_root_identifier, property_name,
        },
        value::{BindingId, BindingVersion, FunctionId},
    },
};

pub(super) mod aliases;
mod analysis;
mod bindings;
mod callbacks;
mod constants;
mod history;
pub(super) mod plan;
mod projection;
mod provenance;
pub(super) mod traversal;
pub(super) mod visitor;

use bindings::{for_each_import_binding, for_each_pat_binding, var_binding_scope};
use plan::ScopePlan;

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

/// Frozen result of source-order scope collection, including conservative
/// shape diagnostics that callers may translate into analysis status.
pub(in crate::analysis) struct ScopedProgram {
    pub(super) graph: FrozenScopeGraph,
    pub(super) issues: Vec<ScopeCollectionIssue>,
}

impl ScopedProgram {
    pub(in crate::analysis) fn into_parts(self) -> (FrozenScopeGraph, Vec<ScopeCollectionIssue>) {
        (self.graph, self.issues)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) enum ScopeCollectionIssue {
    ShapeMismatch,
    UnconsumedShape,
}

#[derive(Debug, Clone)]
/// Assignment of a rooted member/property alias at a source position.
pub(super) struct PropertyAliasAssignment {
    pub(super) span: Span,
    pub(super) scope: ScopeId,
    pub(super) property: SymbolPath,
    pub(super) receiver: swc_ecma_ast::Ident,
    pub(super) target: Option<SymbolPath>,
}

#[derive(Debug, Clone)]
/// Mutation that can invalidate a rooted property provenance.
pub(super) struct RootedPropertyMutation {
    pub(super) span: Span,
    pub(super) scope: ScopeId,
    pub(super) receiver: NamePath,
    pub(super) property: Option<NameId>,
}

/// One scope-forming syntax node recorded during the predeclare pass.
///
/// The shape carries the full structural identity (`scope_id`, `kind`, `span`,
/// `parent`) so the main visitor can locate the matching predeclared scope by
/// key instead of by positional index. Two equal-span siblings of the same
/// parent share the lookup key and are consumed in predeclaration order.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScopeShape {
    scope_id: ScopeId,
    kind: ScopeKind,
    span: Span,
    parent: Option<ScopeId>,
}

/// Structural scope table built during predeclare and consumed during the
/// main visitor pass.
///
/// Replaces the cursor-based plan with a per-key child deque so the two
/// phases share one identity owner. Each entry is recorded once during
/// predeclare; the main visitor pops the next unconsumed child of the
/// current parent for the visited span and kind, with no fallback
/// allocation when the lookup misses.
#[derive(Debug, Default)]
pub(super) struct ScopeShapeTable {
    pub(super) shapes: Vec<ScopeShape>,
    pub(super) children: BTreeMap<ScopeShapeKey, VecDeque<ScopeId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ScopeShapeKey {
    parent: Option<ScopeId>,
    span_lo: BytePos,
    kind: ScopeKind,
}

impl ScopeShapeTable {
    fn new() -> Self {
        Self::default()
    }

    fn record(&mut self, shape: ScopeShape) {
        let key = ScopeShapeKey {
            parent: shape.parent,
            span_lo: shape.span.lo,
            kind: shape.kind,
        };
        self.shapes.push(shape);
        self.children
            .entry(key)
            .or_default()
            .push_back(shape.scope_id);
    }

    fn take_child(
        &mut self,
        parent: Option<ScopeId>,
        span_lo: BytePos,
        kind: ScopeKind,
    ) -> Option<ScopeId> {
        self.children
            .get_mut(&ScopeShapeKey {
                parent,
                span_lo,
                kind,
            })
            .and_then(VecDeque::pop_front)
    }

    #[cfg(test)]
    fn shapes_len(&self) -> usize {
        self.shapes.len()
    }

    #[cfg(test)]
    fn remaining(&self, parent: Option<ScopeId>, span_lo: BytePos, kind: ScopeKind) -> usize {
        self.children
            .get(&ScopeShapeKey {
                parent,
                span_lo,
                kind,
            })
            .map_or(0, VecDeque::len)
    }

    fn is_consumed(&self) -> bool {
        self.children.values().all(VecDeque::is_empty)
    }
}

/// Compact parameter pattern descriptor that avoids cloning SWC Pat ASTs.
#[derive(Debug, Clone)]
pub(super) enum CompactPat {
    Ident(SmolStr),
    Assign(Box<Self>),
    Object(BTreeMap<SmolStr, Self>),
    Array,
    Rest(Box<Self>),
    Other,
}

fn compact_pat(pattern: &Pat) -> CompactPat {
    match pattern {
        Pat::Ident(ident) => CompactPat::Ident(ident.id.sym.to_smolstr()),
        Pat::Assign(assign) => CompactPat::Assign(Box::new(compact_pat(&assign.left))),
        Pat::Object(object) => {
            let mut props = BTreeMap::new();
            for prop in &object.props {
                match prop {
                    ObjectPatProp::KeyValue(kv) => {
                        if let Some(key) = property_name(&kv.key) {
                            props.insert(key, compact_pat(&kv.value));
                        }
                    }
                    ObjectPatProp::Assign(assign) => {
                        props.insert(
                            assign.key.sym.to_smolstr(),
                            CompactPat::Ident(assign.key.sym.to_smolstr()),
                        );
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
            CompactPat::Object(props)
        }
        Pat::Array(_) => CompactPat::Array,
        Pat::Rest(rest) => CompactPat::Rest(Box::new(compact_pat(&rest.arg))),
        Pat::Invalid(_) | Pat::Expr(_) => CompactPat::Other,
    }
}

impl ScopeCollector<'_> {
    #[cfg(test)]
    pub(super) fn from_plan_for_test(plan: ScopePlan) -> ScopeCollector<'static> {
        Self::from_plan(plan, Box::leak(Box::new(SemanticBudget::default())))
    }

    pub(super) fn from_plan(plan: ScopePlan, budget: &SemanticBudget) -> ScopeCollector<'_> {
        ScopeCollector {
            scopes: plan.scopes,
            stack: vec![0],
            assignments: Vec::new(),
            latest_assignments: AssignmentHistory::new(),
            property_assignments: Vec::new(),
            rooted_property_mutations: Vec::new(),
            dynamic_evals: Vec::new(),
            function_scopes: HashMap::new(),
            function_aliases: HashMap::new(),
            calls: Vec::new(),
            inline_parameters: HashMap::new(),
            mutable_static_objects: HashSet::new(),
            pending_function_names: HashMap::new(),
            names: plan.names,
            name_exhausted: plan.name_exhausted,
            version_counters: HashMap::new(),
            scope_shapes: plan.scope_shapes,
            scope_issues: Vec::new(),
            budget,
            #[cfg(test)]
            scope_lookups: 0,
        }
    }

    /// Build a scope-start-order index from the scope vector.
    fn sorted_scope_starts(scopes: &[LexicalScope]) -> Vec<ScopeId> {
        let mut scopes_by_start: Vec<_> = (0..scopes.len()).map(ScopeId::from).collect();
        scopes_by_start.sort_by_key(|index| {
            let scope = &scopes[index.index()];
            (scope.span.lo, scope.depth)
        });
        scopes_by_start
    }

    /// Assign stable binding and function IDs across all scopes.
    fn allocate_ids(
        scopes: &[LexicalScope],
    ) -> (HashMap<ScopedName, BindingId>, Vec<Option<FunctionId>>) {
        let mut binding_ids = HashMap::new();
        let mut next_binding = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            let scope = ScopeId::from(scope);
            for name in lexical_scope.bindings.keys() {
                binding_ids.insert(ScopedName::new(scope, *name), BindingId(next_binding));
                next_binding = next_binding.saturating_add(1);
            }
        }

        let mut function_ids = vec![None; scopes.len()];
        let mut next_function = 0u32;
        for (scope, lexical_scope) in scopes.iter().enumerate() {
            if matches!(lexical_scope.kind, ScopeKind::Program | ScopeKind::Function) {
                function_ids[scope] = Some(FunctionId(next_function));
                next_function = next_function.saturating_add(1);
            }
        }

        (binding_ids, function_ids)
    }

    /// Freeze collected facts into the immutable query graph.
    ///
    /// ID allocation, normalization, and post-collection property indexing
    /// belong to this transition so callers cannot observe a partially built
    /// graph or pass the collector's storage around independently.
    pub(super) fn freeze(mut self, environment: &Environment) -> ScopedProgram {
        if !self.scope_shapes.is_consumed() {
            self.scope_issues
                .push(ScopeCollectionIssue::UnconsumedShape);
        }
        let scope_shape_valid = self.scope_issues.is_empty();
        let issues = std::mem::take(&mut self.scope_issues);
        let parameter_aliases_by_scope = self.parameter_aliases();
        let scopes_by_start = Self::sorted_scope_starts(&self.scopes);
        let assignments =
            FrozenAssignmentIndex::from_assignments(std::mem::take(&mut self.assignments));
        let (binding_ids, function_ids) = Self::allocate_ids(&self.scopes);

        let function_bindings = self
            .function_scopes
            .iter()
            .filter_map(|((scope, name), (function_scope, _))| {
                function_ids
                    .get(function_scope.index())
                    .and_then(|&opt| opt)
                    .map(|function| (Self::scoped_name_by_id(*scope, *name), function))
            })
            .collect();
        let function_aliases = self
            .function_aliases
            .into_iter()
            .filter_map(|(key, function_scope)| {
                function_ids
                    .get(function_scope.index())
                    .and_then(|&opt| opt)
                    .map(|function| (key, function))
            })
            .collect();
        let parameter_aliases = parameter_aliases_by_scope
            .into_iter()
            .filter_map(|(key, provenance)| {
                function_ids
                    .get(key.scope().index())
                    .and_then(|&opt| opt)
                    .map(|function| ((function, key.name()), provenance))
            })
            .collect();

        let property_assignments = self.property_assignments;
        let rooted_mutations = self.rooted_property_mutations;
        let dynamic_evals = self.dynamic_evals;
        let mut graph = ScopeGraph::from_parts(ScopeGraphParts {
            environment: environment.clone(),
            names: self.names,
            scopes: self.scopes,
            scopes_by_start,
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            parameter_aliases,
            mutable_static_objects: self.mutable_static_objects,
            scope_shape_valid,
        });
        graph.finish_collected_properties(property_assignments, rooted_mutations, dynamic_evals);
        let frozen = graph.freeze();
        ScopedProgram {
            graph: frozen,
            issues,
        }
    }

    /// Return the innermost scope currently being traversed.
    fn current_scope(&self) -> ScopeId {
        ScopeId::from(self.stack.last().copied().unwrap_or(0))
    }

    /// Bundlers emit these wrappers around CommonJS imports. They are
    /// recognized only while the wrapper name is itself unbound; a local
    /// function with the same spelling must remain local.
    fn is_module_interop_wrapper(name: &str) -> bool {
        matches!(
            name,
            "__toESM"
                | "__importStar"
                | "__importDefault"
                | "_interopRequireWildcard"
                | "_interopRequireDefault"
        )
    }

    /// Map a declaration kind to its lexical or function binding scope.
    fn binding_scope(&self, kind: VarDeclKind) -> ScopeId {
        if kind != VarDeclKind::Var {
            return self.current_scope();
        }
        var_binding_scope(&self.stack, &self.scopes)
    }

    /// Insert a declaration's initial provenance into a lexical scope.
    pub fn insert(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: BindingProvenance,
    ) {
        let name = name.into();
        self.budget.try_charge();
        let Ok(name) = self.names.intern(name.as_str()) else {
            self.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        self.scopes[scope.index()].bindings.insert(name, provenance);
    }

    fn intern_provenance_strings(&mut self, provenance: &BindingProvenance) {
        match provenance {
            BindingProvenance::StaticString(value) => {
                self.budget.try_charge();
                if self.names.intern(value.as_str()).is_err() {
                    self.name_exhausted = true;
                }
            }
            BindingProvenance::StaticStringArray(values) => {
                for value in values {
                    self.budget.try_charge();
                    if self.names.intern(value.as_str()).is_err() {
                        self.name_exhausted = true;
                    }
                }
            }
            _ => {}
        }
    }

    /// Insert all bindings from an import declaration into `scope`.
    ///
    /// Import handling remains centralized so declaration and source-order
    /// logic use the same provenance construction.
    pub(super) fn insert_import(&mut self, scope: ScopeId, import: &ImportDecl) {
        for_each_import_binding(import, |name, provenance| {
            self.insert(scope, name, provenance);
        });
    }

    fn name_id(&self, name: &str) -> Option<glass_lint_datastructures::NameId> {
        self.names.lookup(name)
    }

    pub(super) fn interned_name(&self, name: &str) -> Option<glass_lint_datastructures::NameId> {
        self.names.lookup(name)
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.lookup_path(path)
    }

    pub(super) fn rooted_name_path(&self, expr: &Expr) -> Option<NamePath> {
        self.rooted_expr_name(expr)
            .and_then(|path| self.name_path(&path))
    }

    pub(super) fn append_name_path(&self, path: &NamePath, segment: &str) -> Option<NamePath> {
        let id = self.names.lookup(segment)?;
        Some(path.append_path(&NamePath::from_ids([id])))
    }

    pub(super) fn scoped_name(&self, scope: ScopeId, name: &str) -> Option<ScopedName> {
        self.names
            .lookup(name)
            .map(|name| ScopedName::new(scope, name))
    }

    pub(super) fn scoped_name_by_id(scope: ScopeId, name: NameId) -> ScopedName {
        ScopedName::new(scope, name)
    }

    fn insert_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    /// Append a source-ordered assignment version and update latest state.
    pub fn record_assignment(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: &str,
        provenance: BindingProvenance,
    ) {
        self.budget.try_charge();
        let Ok(name_id) = self.names.intern(name) else {
            self.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        let next = self.version_counters.entry((scope, name_id)).or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion(*next);
        self.latest_assignments
            .record(&self.names, scope, name, provenance.clone());
        self.assignments.push(AliasAssignment {
            span,
            scope,
            name: name_id,
            version,
            provenance,
        });
    }

    /// Enter the planned scope matching the current source-order node.
    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        let parent = self.current_scope();
        if let Some(scope_id) = self.scope_shapes.take_child(Some(parent), span.lo, kind) {
            self.stack.push(scope_id.index());
            #[cfg(test)]
            {
                self.scope_lookups += 1;
            }
        } else {
            self.scope_issues.push(ScopeCollectionIssue::ShapeMismatch);
        }
    }

    /// Leave the current nested scope without popping the program scope.
    fn pop_scope(&mut self) {
        if self.stack.len() <= 1 {
            debug_assert!(false, "attempted to pop the program scope");
            return;
        }
        let _ = self.stack.pop();
    }

    /// Register every binding introduced by a declaration pattern as local.
    fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| self.insert_local(scope, binding));
    }

    fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        // Prefer assignments over declarations inside each scope: while
        // collecting source order, `latest_assignments` is exactly the state
        // visible at the current AST position.
        for scope in self.stack.iter().rev().copied().map(ScopeId::from) {
            if let Some(assignment) = self.latest_assignments.get(&self.names, scope, name) {
                return Some(assignment);
            }
            if let Some(binding) = self
                .name_id(name)
                .and_then(|name| self.scopes[scope.index()].bindings.get(&name))
            {
                return Some(binding);
            }
        }
        None
    }

    fn visible_binding_scope(&self, name: &str) -> Option<ScopeId> {
        self.stack
            .iter()
            .rev()
            .copied()
            .map(ScopeId::from)
            .find(|scope| {
                self.latest_assignments.contains(&self.names, *scope, name)
                    || self
                        .name_id(name)
                        .is_some_and(|name| self.scopes[scope.index()].bindings.contains_key(&name))
            })
    }

    fn is_unbound(&self, name: &str) -> bool {
        self.scope_issues.is_empty() && self.visible_binding(name).is_none()
    }

    fn rooted_expr_name(&self, expr: &Expr) -> Option<SymbolPath> {
        rooted_expr_chain_with(self, expr)
    }

    fn invalidate_member_root(&mut self, member: &swc_ecma_ast::MemberExpr, span: Span) {
        let Some(root) = member_root_identifier(member) else {
            return;
        };
        if !matches!(
            self.visible_binding(root.sym.as_ref()),
            Some(
                BindingProvenance::StaticStringArray(_)
                    | BindingProvenance::StaticObjectKeys(_)
                    | BindingProvenance::StaticObjectValues(_)
            )
        ) {
            return;
        }
        let Some(scope) = self.stack.iter().rev().find(|scope| {
            self.name_id(root.sym.as_ref())
                .is_some_and(|name| self.scopes[**scope].bindings.contains_key(&name))
        }) else {
            return;
        };
        self.record_assignment(
            span,
            ScopeId::from(*scope),
            root.sym.as_ref(),
            BindingProvenance::Local,
        );
    }

    /// Copy parameter patterns into the function metadata used by the later
    /// call-site projection pass. Keeping this conversion here makes the
    /// collector's function metadata independent of SWC's parameter wrapper.
    fn function_parameters(function: &Function) -> Vec<CompactPat> {
        function
            .params
            .iter()
            .map(|parameter| compact_pat(&parameter.pat))
            .collect()
    }

    fn arrow_parameters(arrow: &ArrowExpr) -> Vec<CompactPat> {
        arrow.params.iter().map(compact_pat).collect()
    }
}

impl RootedExprContext for ScopeCollector<'_> {
    fn rooted_ident_chain(&self, ident: &swc_ecma_ast::Ident) -> Option<SymbolPath> {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. },
            ) => self.names.resolve_path(target),
            Some(_) => None,
            None => Some(ident.sym.as_ref().into()),
        }
    }

    fn rooted_member_chain(&self, member: &swc_ecma_ast::MemberExpr) -> Option<SymbolPath> {
        if is_function_constructor_member(member)
            && function_prototype_builtin(&member.obj).is_none_or(|name| self.is_unbound(name))
        {
            return Some("Function".into());
        }
        let object = self.rooted_expr_name(&member.obj)?;
        let property = member_property_name(&member.prop)?;
        Some(object.append_chain(&property))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
