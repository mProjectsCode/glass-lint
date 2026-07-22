//! Two-phase collection of conservative lexical and alias facts.
//!
//! The collector first predeclares bindings for visibility and hoisting, then
//! traverses the source in order to collect references, assignments, and
//! mutation.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use history::AssignmentHistory;
use smol_str::{SmolStr, ToSmolStr};
use swc_common::{BytePos, Span};
use swc_ecma_ast::{ArrowExpr, Expr, Function, ImportDecl, ObjectPatProp, Pat, VarDeclKind};
use swc_ecma_visit::VisitWith;

use crate::{
    Environment,
    analysis::{
        name::NameId,
        scope::{
            AliasAssignment, BindingProvenance, LexicalScope, ScopeEffect, ScopeGraph,
            ScopeGraphParts, ScopeId, ScopeKind, ScopedName,
            query::rooted::{RootedExprContext, rooted_expr_chain_with},
        },
        syntax::{
            collect_pat_bindings, function_prototype_builtin, is_function_constructor_member,
            member_property_name, member_root_identifier, module_export_name, property_name,
        },
        value::{BindingId, BindingVersion, FunctionId, NamePath, SymbolPath},
    },
};

pub(super) mod aliases;
mod analysis;
mod callbacks;
mod constants;
mod history;
mod provenance;
pub(super) mod visitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Predeclare,
    Collect,
}

/// Mutable state shared by declaration prepass and source-order collection.
///
/// The prepass establishes lexical binding identity; the normal visitor then
/// reuses that scope tree while recording assignments and supported
/// provenance at each use position.
pub(super) struct LexicalScopeCollector<'a> {
    pass: Pass,
    /// Lexical scopes in predeclaration/traversal order.
    pub(super) scopes: Vec<LexicalScope>,
    /// Current lexical path during AST traversal.
    stack: Vec<usize>,
    /// Assignment events retain source order for use-position provenance.
    pub(super) assignments: Vec<AliasAssignment>,
    /// Latest use-position assignment state per lexical scope.
    latest_assignments: AssignmentHistory<'a>,
    /// Property writes retained for flow-aware rooted-member queries.
    pub(super) property_assignments: Vec<PropertyAliasAssignment>,
    /// Writes that invalidate a rooted receiver/property identity.
    pub(super) rooted_property_mutations: Vec<RootedPropertyMutation>,
    /// Dynamic `eval` sites that make local provenance conservative.
    pub(super) dynamic_evals: Vec<(ScopeId, ScopeEffect)>,
    /// Function scopes and their parameter patterns by visible NameId.
    pub(super) function_scopes: BTreeMap<(ScopeId, NameId), (ScopeId, Vec<CompactPat>)>,
    /// Aliases that point to a locally declared helper function.
    pub(super) function_aliases: BTreeMap<ScopedName, ScopeId>,
    /// Calls retained for the later, scope-aware helper parameter pass.
    calls: Vec<(ScopeId, NameId, Vec<Option<BindingProvenance>>)>,
    /// Proven callback arguments installed when an inline function is entered.
    inline_parameters: BTreeMap<BytePos, BTreeMap<SmolStr, BindingProvenance>>,
    /// `var`-bound objects whose mutation prevents constant projection.
    pub(super) mutable_static_objects: BTreeSet<ScopedName>,
    names: crate::analysis::name::NameTableCtx<'a>,
    pub(super) name_exhausted: bool,
    /// Per (scope, name) counter to avoid rescanniing all assignments.
    version_counters: BTreeMap<(ScopeId, NameId), u32>,
    /// Structural scope shape table built during predeclare and consumed by
    /// the main visitor, replacing positional index–based synchronization.
    scope_shapes: ScopeShapeTable,
    /// A phase mismatch is a conservative incomplete analysis, not a panic.
    scope_diverged: bool,
    #[cfg(test)]
    scope_lookups: usize,
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
struct ScopeShape {
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
struct ScopeShapeTable {
    shapes: Vec<ScopeShape>,
    children: BTreeMap<ScopeShapeKey, VecDeque<ScopeId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ScopeShapeKey {
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

impl<'a> LexicalScopeCollector<'a> {
    /// Initialize an empty collector with a program-level lexical scope.
    #[cfg(test)]
    pub fn new(program_span: Span) -> Self {
        Self::with_names(program_span, crate::analysis::name::NameTableCtx::testing())
    }

    pub(super) fn with_names(
        program_span: Span,
        names: crate::analysis::name::NameTableCtx<'a>,
    ) -> Self {
        Self {
            pass: Pass::Predeclare,
            scopes: vec![LexicalScope {
                span: program_span,
                depth: 0,
                kind: ScopeKind::Program,
                parent: None,
                bindings: BTreeMap::new(),
            }],
            stack: vec![0],
            assignments: Vec::new(),
            latest_assignments: AssignmentHistory::new(names),
            property_assignments: Vec::new(),
            rooted_property_mutations: Vec::new(),
            dynamic_evals: Vec::new(),
            function_scopes: BTreeMap::new(),
            function_aliases: BTreeMap::new(),
            calls: Vec::new(),
            inline_parameters: BTreeMap::new(),
            mutable_static_objects: BTreeSet::new(),
            names,
            name_exhausted: false,
            version_counters: BTreeMap::new(),
            scope_shapes: ScopeShapeTable::new(),
            scope_diverged: false,
            #[cfg(test)]
            scope_lookups: 0,
        }
    }

    /// Predeclare bindings before source-order collection.
    ///
    /// JavaScript lexical bindings are visible for the whole lexical scope
    /// (with TDZ handled as an unresolved/local fact), and `var`/function
    /// declarations are hoisted.
    pub fn predeclare(&mut self, program: &swc_ecma_ast::Program) {
        self.pass = Pass::Predeclare;
        program.visit_children_with(self);
        self.scope_diverged = false;
        self.pass = Pass::Collect;
        #[cfg(test)]
        {
            self.scope_lookups = 0;
        }
    }

    /// Predeclare bindings and return the number of scope shapes recorded.
    /// Tests use this to assert one shape per visitor push call.
    #[cfg(test)]
    pub fn predeclare_and_count(&mut self, program: &swc_ecma_ast::Program) -> usize {
        self.predeclare(program);
        self.scope_shapes.shapes_len()
    }

    /// Freeze collected facts into the immutable query graph.
    ///
    /// ID allocation, normalization, and post-collection property indexing
    /// belong to this transition so callers cannot observe a partially built
    /// graph or pass the collector's storage around independently.
    pub(super) fn freeze(mut self, environment: &Environment) -> ScopeGraph<'a> {
        let parameter_aliases_by_scope = self.parameter_aliases();
        let mut scopes_by_start = (0..self.scopes.len())
            .map(ScopeId::from)
            .collect::<Vec<_>>();
        scopes_by_start.sort_by_key(|index| {
            let scope = &self.scopes[index.index()];
            (scope.span.lo, scope.depth)
        });

        let mut assignments = BTreeMap::<
            ScopeId,
            BTreeMap<crate::analysis::name::NameId, Vec<AliasAssignment>>,
        >::new();
        let collected_assignments = std::mem::take(&mut self.assignments);
        for assignment in collected_assignments {
            assignments
                .entry(assignment.scope)
                .or_default()
                .entry(assignment.name)
                .or_default()
                .push(assignment);
        }
        for scope_assignments in assignments.values_mut() {
            for binding_assignments in scope_assignments.values_mut() {
                binding_assignments.sort_by_key(|assignment| assignment.span.lo);
            }
        }

        let mut binding_ids = BTreeMap::new();
        let mut next_binding_id = 0u32;
        for (scope, lexical_scope) in self.scopes.iter().enumerate() {
            let scope = ScopeId::from(scope);
            for name in lexical_scope.bindings.keys() {
                binding_ids.insert(ScopedName::new(scope, *name), BindingId(next_binding_id));
                next_binding_id = next_binding_id.saturating_add(1);
            }
        }

        let mut function_ids = BTreeMap::new();
        let mut next_function_id = 0u32;
        for (scope, lexical_scope) in self.scopes.iter().enumerate() {
            let scope = ScopeId::from(scope);
            if matches!(lexical_scope.kind, ScopeKind::Program | ScopeKind::Function) {
                function_ids.insert(scope, FunctionId(next_function_id));
                next_function_id = next_function_id.saturating_add(1);
            }
        }

        let function_bindings = self
            .function_scopes
            .iter()
            .filter_map(|((scope, name), (function_scope, _))| {
                function_ids
                    .get(function_scope)
                    .copied()
                    .map(|function| (Self::scoped_name_by_id(*scope, *name), function))
            })
            .collect();
        let function_aliases = self
            .function_aliases
            .into_iter()
            .filter_map(|(key, function_scope)| {
                function_ids
                    .get(&function_scope)
                    .copied()
                    .map(|function| (key, function))
            })
            .collect();
        let parameter_aliases = parameter_aliases_by_scope
            .into_iter()
            .filter_map(|(key, provenance)| {
                function_ids
                    .get(&key.scope())
                    .copied()
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
        });
        graph.finish_collected_properties(property_assignments, rooted_mutations, dynamic_evals);
        graph
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
        // `var` is function-scoped, unlike `let` and `const`, so skip nested
        // blocks until the enclosing function or program scope is reached.
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|index| {
                matches!(
                    self.scopes[*index].kind,
                    ScopeKind::Program | ScopeKind::Function
                )
            })
            .map_or_else(|| ScopeId::from(0), ScopeId::from)
    }

    /// Insert a declaration's initial provenance into a lexical scope.
    pub fn insert(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: BindingProvenance,
    ) {
        let name = name.into();
        let Ok(name) = self.names.intern(name.as_str()) else {
            self.name_exhausted = true;
            return;
        };
        self.scopes[scope.index()].bindings.insert(name, provenance);
    }

    /// Insert all bindings from an import declaration into `scope`.
    ///
    /// Shared by the predeclare and main-visitor passes so import-handling
    /// logic has a single maintenance point.
    pub(super) fn insert_import(&mut self, scope: ScopeId, import: &ImportDecl) {
        let import_module = import.src.value.to_string_lossy().to_smolstr();
        for specifier in &import.specifiers {
            match specifier {
                swc_ecma_ast::ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_smolstr();
                    let export = named
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), module_export_name);
                    self.insert(
                        scope,
                        local,
                        BindingProvenance::ModuleExport {
                            module: import_module.clone(),
                            export,
                        },
                    );
                }
                swc_ecma_ast::ImportSpecifier::Namespace(namespace) => self.insert(
                    scope,
                    namespace.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: import_module.clone(),
                    },
                ),
                swc_ecma_ast::ImportSpecifier::Default(default) => self.insert(
                    scope,
                    default.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: import_module.clone(),
                    },
                ),
            }
        }
    }

    fn name_id(&self, name: &str) -> Option<crate::analysis::name::NameId> {
        self.names.lookup(name)
    }

    pub(super) fn interned_name(&self, name: &str) -> Option<crate::analysis::name::NameId> {
        self.names.intern(name).ok()
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.intern_path(path)
    }

    pub(super) fn rooted_name_path(&self, expr: &Expr) -> Option<NamePath> {
        self.rooted_expr_name(expr)
            .and_then(|path| self.name_path(&path))
    }

    pub(super) fn append_name_path(&self, path: &NamePath, segment: &str) -> Option<NamePath> {
        let id = self.names.intern(segment).ok()?;
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
        let Ok(name_id) = self.names.intern(name) else {
            self.name_exhausted = true;
            return;
        };
        let next = self.version_counters.entry((scope, name_id)).or_insert(0);
        *next = next.saturating_add(1);
        let version = BindingVersion(*next);
        self.latest_assignments
            .record(scope, name, provenance.clone());
        self.assignments.push(AliasAssignment {
            span,
            scope,
            name: name_id,
            version,
            provenance,
        });
    }

    /// Enter a scope owned by the predeclare phase.
    ///
    /// The predeclare pass allocates the scope and records its shape. The
    /// collect pass resolves the visited scope by structural identity
    /// (`parent`, `span`, `kind`) against the shape table and pushes that
    /// predeclared scope. A miss means the two phases walked the same
    /// scope-forming syntax differently; the artifact is marked diverged and
    /// the visitor stays in the current scope instead of allocating a
    /// fallback that would silently desynchronize the two passes.
    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        if self.pass == Pass::Predeclare {
            let parent = self.current_scope();
            let index = self.scopes.len();
            self.scopes.push(LexicalScope {
                span,
                depth: self.stack.len(),
                kind,
                parent: Some(parent),
                bindings: BTreeMap::new(),
            });
            self.scope_shapes.record(ScopeShape {
                scope_id: ScopeId::from(index),
                kind,
                span,
                parent: Some(parent),
            });
            self.stack.push(index);
            return;
        }
        let parent = self.current_scope();
        if let Some(scope_id) = self.scope_shapes.take_child(Some(parent), span.lo, kind) {
            self.stack.push(scope_id.index());
            #[cfg(test)]
            {
                self.scope_lookups += 1;
            }
        } else {
            self.scope_diverged = true;
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
        let mut bindings = BTreeSet::new();
        collect_pat_bindings(pat, &mut bindings);
        for binding in bindings {
            self.insert_local(scope, binding);
        }
    }

    fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        // Prefer assignments over declarations inside each scope: while
        // collecting source order, `latest_assignments` is exactly the state
        // visible at the current AST position.
        for scope in self.stack.iter().rev().copied().map(ScopeId::from) {
            if let Some(assignment) = self.latest_assignments.get(scope, name) {
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
                self.latest_assignments.contains(*scope, name)
                    || self
                        .name_id(name)
                        .is_some_and(|name| self.scopes[scope.index()].bindings.contains_key(&name))
            })
    }

    fn is_unbound(&self, name: &str) -> bool {
        !self.scope_diverged && self.visible_binding(name).is_none()
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

    fn register_function_expression(&mut self, name: Option<NameId>, expr: &Expr) -> bool {
        let Some(name) = name else {
            return false;
        };
        let declaration_scope = self.current_scope();
        match expr {
            Expr::Arrow(arrow) => {
                let parameters = Self::arrow_parameters(arrow);
                self.push_scope(arrow.span, ScopeKind::Function);
                let scope = self.current_scope();
                for param in &arrow.params {
                    self.insert_pat_locals(scope, param);
                }
                self.function_scopes
                    .insert((declaration_scope, name), (scope, parameters));
                arrow.body.visit_with(self);
                self.pop_scope();
                true
            }
            Expr::Fn(function_expr) => {
                let parameters = Self::function_parameters(&function_expr.function);
                self.push_scope(function_expr.function.span, ScopeKind::Function);
                let scope = self.current_scope();
                if let Some(ident) = &function_expr.ident {
                    self.insert_local(scope, ident.sym.to_string());
                }
                for param in &function_expr.function.params {
                    self.insert_pat_locals(scope, &param.pat);
                }
                self.function_scopes
                    .insert((declaration_scope, name), (scope, parameters));
                function_expr.function.decorators.visit_with(self);
                function_expr.function.body.visit_with(self);
                self.pop_scope();
                true
            }
            Expr::Paren(paren) => self.register_function_expression(Some(name), &paren.expr),
            _ => false,
        }
    }
}

impl RootedExprContext for LexicalScopeCollector<'_> {
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
mod tests {
    use swc_common::Spanned;
    use swc_ecma_visit::VisitWith;

    use super::*;

    fn collect(source: &str) -> LexicalScopeCollector<'_> {
        let parsed = crate::parse(source, "scope-collector.js").expect("source should parse");
        let mut collector = LexicalScopeCollector::new(parsed.program.span());
        let predeclared = collector.predeclare_and_count(&parsed.program);
        parsed.program.visit_children_with(&mut collector);
        assert!(
            !collector.scope_diverged,
            "main visitor did not diverge from predeclared scopes"
        );
        assert_eq!(
            collector.scope_lookups, predeclared,
            "main visitor consumed one shape per predeclared scope",
        );
        collector
    }

    fn scope_fingerprint(collector: &LexicalScopeCollector) -> Vec<String> {
        collector
            .scopes
            .iter()
            .map(|scope| {
                format!(
                    "parent={:?} depth={} kind={:?} span=({}, {}) bindings={:?}",
                    scope.parent,
                    scope.depth,
                    scope.kind,
                    scope.span.lo.0,
                    scope.span.hi.0,
                    scope.bindings
                )
            })
            .collect()
    }

    #[test]
    fn preserves_scope_order_for_all_scope_constructs() {
        let source = r"
            function outer(parameter) {
                { let block = parameter; }
                for (let index = 0; index < 1; index++) {
                    (() => { let nested = index; })();
                }
                for (const item of items) { function loopFunction() {} }
                for (const key in object) { key; }
                switch (parameter) {
                    case 0: { let caseValue = parameter; break; }
                    default: break;
                }
                try { throw parameter; }
                catch (error) { const caught = error; }
                with (context) { value; }
                const functionValue = function named(value) { return value; };
                const arrow = value => { return value; };
            }
        ";
        let first = collect(source);
        let second = collect(source);

        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Function)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Block)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Dynamic)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Function && scope.depth > 2)
        );
    }

    #[test]
    fn reuses_same_span_same_kind_siblings_by_order() {
        let parsed = crate::parse("value;", "same-span.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = LexicalScopeCollector::new(span);

        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        let predeclared = collector.scope_shapes.shapes_len();
        assert_eq!(predeclared, 2);
        collector.scope_diverged = false;
        collector.pass = Pass::Collect;

        collector.push_scope(span, ScopeKind::Block);
        let first = collector.current_scope();
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        let second = collector.current_scope();

        assert_eq!((first, second), (ScopeId::from(1), ScopeId::from(2)));
        assert_eq!(collector.scope_lookups, 2);
        assert_eq!(
            collector
                .scope_shapes
                .remaining(Some(ScopeId::from(0)), span.lo, ScopeKind::Block),
            0,
        );
    }

    fn sibling_scope_lookups(count: usize) -> usize {
        let source = (0..count)
            .map(|index| format!("{{ let value{index} = {index}; }}"))
            .collect::<Vec<_>>()
            .join("\n");
        let collector = collect(&source);
        collector.scope_lookups
    }

    #[test]
    fn many_sibling_scopes_consume_one_shape_each() {
        let one = sibling_scope_lookups(128);
        let two = sibling_scope_lookups(256);

        assert_eq!(one, 128);
        assert_eq!(two, one * 2);
    }

    #[test]
    fn divergence_on_extra_scope_fails_closed() {
        let parsed = crate::parse("value;", "divergence-extra.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = LexicalScopeCollector::new(span);
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        assert_eq!(collector.scope_shapes.shapes_len(), 1);
        collector.scope_diverged = false;
        collector.pass = Pass::Collect;
        let before = collector.current_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        assert!(!collector.scope_diverged);
        assert_eq!(collector.current_scope(), before);
        collector.push_scope(span, ScopeKind::Block);
        assert!(collector.scope_diverged);
        // No fallback scope was allocated during the diverged push.
        assert_eq!(collector.current_scope(), before);
    }

    #[test]
    fn divergence_on_missing_scope_fails_closed() {
        let parsed = crate::parse("value;", "divergence-missing.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = LexicalScopeCollector::new(span);
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        assert_eq!(collector.scope_shapes.shapes_len(), 2);
        collector.scope_diverged = false;
        collector.pass = Pass::Collect;
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        assert!(!collector.scope_diverged);
        assert_eq!(
            collector
                .scope_shapes
                .remaining(Some(ScopeId::from(0)), span.lo, ScopeKind::Block),
            1,
            "the unvisited predeclared shape stays in the table",
        );
        // A second visit consumes the remaining predeclared shape.
        collector.push_scope(span, ScopeKind::Block);
        assert!(!collector.scope_diverged);
        // A third visit finds no matching shape and fails closed.
        let before = collector.current_scope();
        collector.push_scope(span, ScopeKind::Block);
        assert!(collector.scope_diverged);
        // No fallback scope was allocated.
        assert_eq!(collector.current_scope(), before);
    }

    #[test]
    fn divergence_on_kind_mismatch_fails_closed() {
        let parsed = crate::parse("value;", "divergence-kind.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = LexicalScopeCollector::new(span);
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.scope_diverged = false;
        collector.pass = Pass::Collect;
        let before = collector.current_scope();
        collector.push_scope(span, ScopeKind::Function);
        assert!(collector.scope_diverged);
        // The visitor stays in the parent scope; no fallback is allocated.
        assert_eq!(collector.current_scope(), before);
    }

    #[test]
    fn hoisted_var_in_blocks_preserves_function_scoping() {
        let source = r"
            function outer() {
                if (true) { var hoisted = 1; }
                return hoisted;
            }
        ";
        let collector = collect(source);

        let function_scopes: Vec<_> = collector
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.kind == ScopeKind::Function)
            .collect();
        assert_eq!(function_scopes.len(), 1);
        let (fn_idx, fn_scope) = function_scopes[0];
        assert!(
            !fn_scope.bindings.is_empty(),
            "function scope {fn_idx} has no bindings",
        );

        let block_scopes: Vec<_> = collector
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.kind == ScopeKind::Block)
            .collect();
        // var hoisted into function scope means block scopes should not have
        // the hoisted binding
        for (idx, scope) in &block_scopes {
            let is_empty = !scope
                .bindings
                .iter()
                .any(|(_, p)| matches!(p, BindingProvenance::Local));
            assert!(is_empty, "block scope {idx} contains var bindings");
        }
    }

    #[test]
    fn catch_without_param_forms_valid_scope() {
        let source = r"
            try { let a = 1; } catch { let b = 2; }
        ";
        let first = collect(source);
        let second = collect(source);
        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Block && scope.depth == 1)
        );
    }

    #[test]
    fn loops_with_and_without_inits_form_valid_scopes() {
        let source = r"
            for (;;) { break; }
            for (let i = 0; i < 1; i++) { break; }
            for (const x of []) { break; }
            for (const k in {}) { break; }
        ";
        let first = collect(source);
        let second = collect(source);
        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        assert_eq!(
            first
                .scopes
                .iter()
                .filter(|scope| scope.kind == ScopeKind::Block)
                .count(),
            second
                .scopes
                .iter()
                .filter(|scope| scope.kind == ScopeKind::Block)
                .count()
        );
    }

    #[test]
    fn with_statement_creates_dynamic_scope() {
        let source = r"
            const obj = {};
            with (obj) { let value = prop; }
        ";
        let first = collect(source);
        let second = collect(source);
        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Dynamic)
        );
    }

    #[test]
    fn switch_with_cases_forms_block_scope() {
        let source = r"
            switch (a) { case 0: { let b = 1; break; } default: break; }
        ";
        let first = collect(source);
        let second = collect(source);
        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        // Switch body is a block scope
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Block && scope.depth == 1)
        );
    }

    #[test]
    fn nested_function_and_arrow_scopes_have_correct_depths() {
        let source = r"
            function a() {
                function b() {
                    const c = () => { return 1; };
                    c();
                }
                b();
            }
        ";
        let collector = collect(source);
        let function_depths: Vec<_> = collector
            .scopes
            .iter()
            .filter(|scope| scope.kind == ScopeKind::Function)
            .map(|scope| scope.depth)
            .collect();
        // Function bodies have intervening block scopes:
        // depth 1 = a, depth 3 = b (after a-block), depth 5 = arrow c (after a-block +
        // b-block)
        assert!(function_depths.contains(&1));
        assert!(function_depths.contains(&3));
        assert!(function_depths.contains(&5));
    }

    #[test]
    fn predeclare_and_collect_phases_produce_identical_scopes() {
        let source = r"
            function outer(p1, p2) {
                const value = p1 + p2;
                for (const item of [1,2,3]) {
                    const doubled = item * 2;
                }
                try { throw value; }
                catch (error) {
                    const message = error.toString();
                }
                if (value) {
                    const flag = true;
                } else {
                    const flag = false;
                }
                const helper = (x) => x + 1;
                helper(value);
            }
        ";
        let first = collect(source);
        let second = collect(source);
        assert_eq!(first.scopes.len(), second.scopes.len());
        for (i, (a, b)) in first.scopes.iter().zip(second.scopes.iter()).enumerate() {
            assert_eq!(
                a.kind, b.kind,
                "scope {i} kind differs: {:?} vs {:?}",
                a.kind, b.kind
            );
            assert_eq!(a.depth, b.depth, "scope {i} depth differs");
            assert_eq!(a.parent, b.parent, "scope {i} parent differs");
            assert_eq!(
                a.bindings.keys().collect::<Vec<_>>(),
                b.bindings.keys().collect::<Vec<_>>(),
                "scope {i} binding keys differ",
            );
        }
    }

    #[test]
    fn structural_lookup_distinguishes_equal_span_siblings_at_different_parents() {
        let source = r"
            { let outer = 1; }
            function f() { { let inner = 1; } }
        ";
        let collector = collect(source);

        let (program_block_index, program_block) = collector
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind == ScopeKind::Block && scope.parent == Some(ScopeId::from(0))
            })
            .expect("outer block under program");
        let (function_index, _function_scope) = collector
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind == ScopeKind::Function && scope.parent == Some(ScopeId::from(0))
            })
            .expect("function under program");
        let (inner_block_index, inner_block) = collector
            .scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| {
                scope.kind == ScopeKind::Block
                    && scope.parent == Some(ScopeId::from(function_index))
            })
            .expect("inner block under function");

        // Both blocks share a Span layout but have different parents; the
        // structural lookup must keep them distinct.
        assert_ne!(program_block_index, inner_block_index);
        assert_eq!(program_block.parent, Some(ScopeId::from(0)));
        assert_eq!(inner_block.parent, Some(ScopeId::from(function_index)));
    }

    #[test]
    fn structural_lookup_resolves_visitor_pushes_without_positional_synchronization() {
        let source = r"
            function outer() {
                for (let i = 0; i < 1; i++) {
                    try { throw i; } catch (e) { const v = e; }
                }
                with (context) { const w = prop; }
                const arrow = () => { return 1; };
            }
        ";
        let collector = collect(source);
        assert!(
            !collector.scope_diverged,
            "no divergence when the visitor walks scope-forming syntax in predeclaration order",
        );
        assert_eq!(
            collector.scope_lookups,
            collector.scope_shapes.shapes_len(),
            "every predeclared shape was consumed by one visitor push",
        );
    }

    #[test]
    fn deliberate_walker_divergence_fails_closed_without_fallback_allocation() {
        // Predeclare 3 sibling Block scopes under the program scope.
        let parsed = crate::parse("value;", "walker-divergence.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = LexicalScopeCollector::new(span);
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        let predeclared = collector.scope_shapes.shapes_len();
        assert_eq!(predeclared, 3);
        collector.scope_diverged = false;
        collector.pass = Pass::Collect;

        // Walk the predeclared shapes in reversed order: a structural
        // identity lookup must still resolve each push correctly because
        // the lookup is keyed by (parent, span, kind), not by position.
        let program = ScopeId::from(0);
        let remaining_first =
            collector
                .scope_shapes
                .remaining(Some(program), span.lo, ScopeKind::Block);
        assert_eq!(remaining_first, 3);
        collector.push_scope(span, ScopeKind::Block);
        let first = collector.current_scope();
        collector.pop_scope();
        assert!(!collector.scope_diverged);
        collector.push_scope(span, ScopeKind::Block);
        let second = collector.current_scope();
        collector.pop_scope();
        assert!(!collector.scope_diverged);
        collector.push_scope(span, ScopeKind::Block);
        let third = collector.current_scope();
        collector.pop_scope();
        assert!(!collector.scope_diverged);
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_ne!(first, third);
        assert_eq!(
            collector.scope_lookups, 3,
            "every predeclared shape was consumed",
        );

        // A visit that is not preceded by a matching predeclared shape
        // must fail closed without allocating a fallback scope.
        let before = collector.current_scope();
        collector.push_scope(span, ScopeKind::Block);
        assert!(collector.scope_diverged);
        assert_eq!(
            collector.current_scope(),
            before,
            "divergence leaves the visitor in the parent scope",
        );
    }
}
