//! Two-phase collection of conservative lexical and alias facts.
//!
//! The collector first predeclares bindings for visibility and hoisting, then
//! traverses the source in order to collect references, assignments, and
//! mutation.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use std::collections::{BTreeMap, BTreeSet};

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
mod callbacks;
mod constants;
mod history;
mod predeclare;
mod provenance;
mod visitor;

/// Mutable state shared by declaration prepass and source-order collection.
///
/// The prepass establishes lexical binding identity; the normal visitor then
/// reuses that scope tree while recording assignments and supported
/// provenance at each use position.
pub(super) struct LexicalScopeCollector<'a> {
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
    reuse_scopes: bool,
    /// Typed scope plan built during predeclare and consumed by the main
    /// visitor, replacing positional index–based synchronization.
    scope_plan: ScopePlan,
    /// A phase mismatch is a conservative incomplete analysis, not a panic.
    scope_diverged: bool,
    #[cfg(test)]
    scope_reuse_steps: usize,
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
/// Each entry maps to one `push_scope` call during predeclare. The main
/// visitor consumes the plan in order to locate the correct predeclared scope
/// for each `push_scope`, replacing the previous positional index–based
/// synchronization.
#[derive(Debug, Clone, Copy)]
struct ScopePlanEntry {
    scope_index: usize,
}

/// Ordered record of scope structure built during predeclare and validated
/// during the main visitor pass.
#[derive(Debug)]
struct ScopePlan {
    entries: Vec<ScopePlanEntry>,
    cursor: usize,
}

impl ScopePlan {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
        }
    }

    fn push(&mut self, scope_index: usize) {
        self.entries.push(ScopePlanEntry { scope_index });
    }

    fn advance(&mut self) -> Option<&ScopePlanEntry> {
        let entry = self.entries.get(self.cursor);
        self.cursor = self.cursor.saturating_add(1);
        entry
    }

    #[cfg(test)]
    fn is_at_end(&self) -> bool {
        self.cursor >= self.entries.len()
    }

    #[cfg(test)]
    fn entries_len(&self) -> usize {
        self.entries.len()
    }

    fn reset(&mut self) {
        self.cursor = 0;
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
            reuse_scopes: false,
            scope_plan: ScopePlan::new(),
            scope_diverged: false,
            #[cfg(test)]
            scope_reuse_steps: 0,
        }
    }

    /// Predeclare bindings before source-order collection.
    ///
    /// JavaScript lexical bindings are visible for the whole lexical scope
    /// (with TDZ handled as an unresolved/local fact), and `var`/function
    /// declarations are hoisted.
    pub fn predeclare(&mut self, program: &swc_ecma_ast::Program) {
        let mut visitor = predeclare::PredeclareVisitor { collector: self };
        program.visit_children_with(&mut visitor);
        self.reuse_scopes = true;
        self.scope_plan.reset();
        self.scope_diverged = false;
        #[cfg(test)]
        {
            self.scope_reuse_steps = 0;
        }
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

    /// Enter a predeclared scope, conservatively handling phase divergence.
    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        if self.reuse_scopes {
            let parent = self.current_scope();
            if let Some(entry) = self.scope_plan.advance() {
                let index = entry.scope_index;
                if self.scopes[index].parent == Some(parent)
                    && self.scopes[index].span == span
                    && self.scopes[index].kind == kind
                {
                    self.stack.push(index);
                } else {
                    self.scope_diverged = true;
                    let index = self.scopes.len();
                    self.scopes.push(LexicalScope {
                        span,
                        depth: self.stack.len(),
                        kind,
                        parent: Some(parent),
                        bindings: BTreeMap::new(),
                    });
                    self.stack.push(index);
                }
            } else {
                self.scope_diverged = true;
                let index = self.scopes.len();
                self.scopes.push(LexicalScope {
                    span,
                    depth: self.stack.len(),
                    kind,
                    parent: Some(parent),
                    bindings: BTreeMap::new(),
                });
                self.stack.push(index);
            }
            #[cfg(test)]
            {
                self.scope_reuse_steps += 1;
            }
            return;
        }
        let index = self.scopes.len();
        let parent = self.current_scope();
        self.scopes.push(LexicalScope {
            span,
            depth: self.stack.len(),
            kind,
            parent: Some(parent),
            bindings: BTreeMap::new(),
        });
        self.scope_plan.push(index);
        self.stack.push(index);
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
        collector.predeclare(&parsed.program);
        parsed.program.visit_children_with(&mut collector);
        assert!(
            collector.scope_plan.is_at_end(),
            "main visitor consumed all scope plan entries"
        );
        assert_eq!(
            collector.scope_reuse_steps,
            collector.scope_plan.entries_len()
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
        collector.reuse_scopes = true;
        collector.scope_plan.reset();

        collector.push_scope(span, ScopeKind::Block);
        let first = collector.current_scope();
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        let second = collector.current_scope();

        assert_eq!((first, second), (ScopeId::from(1), ScopeId::from(2)));
        assert_eq!(collector.scope_reuse_steps, 2);
    }

    fn sibling_scope_steps(count: usize) -> usize {
        let source = (0..count)
            .map(|index| format!("{{ let value{index} = {index}; }}"))
            .collect::<Vec<_>>()
            .join("\n");
        let collector = collect(&source);
        collector.scope_reuse_steps
    }

    #[test]
    fn many_sibling_scopes_use_one_cursor_step_each() {
        let one = sibling_scope_steps(128);
        let two = sibling_scope_steps(256);

        assert_eq!(one, 128);
        assert_eq!(two, one * 2);
    }
}
