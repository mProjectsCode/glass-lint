//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.

use std::collections::BTreeMap;

use swc_common::{Span, Spanned};
use swc_ecma_ast::{Expr, Ident, MemberExpr, Program};
use swc_ecma_visit::VisitWith;

use super::ast::{SymbolCallProvenance, SymbolMemberProvenance, member_root_ident};
use super::constant::{self, ConstValue};
use super::value::{BindingId, BindingKey, BindingRoot, BindingVersion, FunctionId, SymbolPath};
use collector::AliasCollector;
use collector_helpers::contains;

mod collector;
mod collector_helpers;
mod constants;
mod member;
mod rooted;
use rooted::rooted_expr_chain_with;

#[derive(Debug, Default, Clone)]
pub(super) struct ScopeGraph {
    scopes: Vec<AliasScope>,
    scopes_by_start: Vec<usize>,
    assignments: BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    binding_ids: BTreeMap<(usize, String), BindingId>,
    function_ids: BTreeMap<usize, FunctionId>,
    function_bindings: BTreeMap<(usize, String), FunctionId>,
    function_aliases: BTreeMap<(usize, String), FunctionId>,
    property_assignments: BTreeMap<(BindingKey, Vec<String>), Vec<PropertyAliasFact>>,
    parameter_aliases: BTreeMap<(FunctionId, String), BindingProvenance>,
    dynamic_evals: Vec<(usize, Span)>,
    mutable_static_objects: std::collections::BTreeSet<(usize, String)>,
}

#[derive(Debug, Clone)]
pub(super) struct AliasScope {
    span: Span,
    depth: usize,
    kind: ScopeKind,
    pub(super) parent: Option<usize>,
    bindings: BTreeMap<String, BindingProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Program,
    Function,
    Block,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BindingProvenance {
    Local,
    ValueAlias {
        target: SymbolPath,
    },
    BoundCallable {
        target: SymbolPath,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    BoundModuleCallable {
        module: String,
        export: String,
        bound_arguments: Vec<Option<BoundArgument>>,
    },
    ReturnedObject {
        source: SymbolPath,
    },
    ModuleExport {
        module: String,
        export: String,
    },
    ModuleNamespace {
        module: String,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(Vec<String>),
    StaticObjectValues(BTreeMap<String, SymbolPath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BoundArgument {
    StaticString(String),
    RootedExpression(SymbolPath),
}

/// The collection boundary between lexical analysis and value interning.
///
/// Scope collection may use its compact declaration/assignment representation
/// internally, but the resolver receives one typed snapshot for each node. It
/// therefore does not need to reinterpret `BindingProvenance` while building
/// the authoritative value arena.
#[derive(Debug, Clone)]
pub(super) struct IdentValueSeed {
    pub(super) call: SymbolCallProvenance,
    pub(super) rooted_chain: Option<SymbolPath>,
    pub(super) binding: Option<BindingKey>,
    pub(super) constant: ConstValue,
    pub(super) bound_arguments: Option<Vec<Option<BoundArgument>>>,
}

#[derive(Debug, Clone)]
pub(super) struct MemberValueSeed {
    pub(super) syntactic_chain: Option<SymbolPath>,
    pub(super) rooted_chain: Option<SymbolPath>,
    pub(super) binding: Option<BindingKey>,
    pub(super) module_member: Option<SymbolMemberProvenance>,
    pub(super) returned_member: Option<(SymbolPath, String)>,
}

#[derive(Debug, Clone)]
struct AliasAssignment {
    span: Span,
    scope: usize,
    name: String,
    version: BindingVersion,
    provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
struct PropertyAliasFact {
    span: Span,
    scope: usize,
    target: Option<SymbolPath>,
}

impl ScopeGraph {
    pub(super) fn collect(program: &Program) -> Self {
        let mut collector = AliasCollector::new(program.span());
        // Build declarations before collecting initializers and uses.  This
        // makes the resolver position-aware without making it traversal-order
        // dependent: an earlier use of a later declaration is local/TDZ, not
        // an accidentally unshadowed global.
        collector.predeclare(program);
        program.visit_children_with(&mut collector);
        let parameter_aliases_by_scope = collector.parameter_aliases();
        // Scope lookup starts from the latest opening delimiter, then walks to
        // parents only when the candidate does not contain the queried span.
        let mut scopes_by_start = (0..collector.scopes.len()).collect::<Vec<_>>();
        scopes_by_start.sort_by_key(|index| {
            let scope = &collector.scopes[*index];
            (scope.span.lo, scope.depth)
        });
        let mut assignments = BTreeMap::<usize, BTreeMap<String, Vec<AliasAssignment>>>::new();
        for assignment in collector.assignments {
            assignments
                .entry(assignment.scope)
                .or_default()
                .entry(assignment.name.clone())
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
        for (scope, lexical_scope) in collector.scopes.iter().enumerate() {
            for name in lexical_scope.bindings.keys() {
                binding_ids.insert((scope, name.clone()), BindingId(next_binding_id));
                next_binding_id = next_binding_id.saturating_add(1);
            }
        }
        let mut function_ids = BTreeMap::new();
        let mut next_function_id = 0u32;
        for (scope, lexical_scope) in collector.scopes.iter().enumerate() {
            if matches!(lexical_scope.kind, ScopeKind::Program | ScopeKind::Function) {
                function_ids.insert(scope, FunctionId(next_function_id));
                next_function_id = next_function_id.saturating_add(1);
            }
        }
        let function_bindings = collector
            .function_scopes
            .iter()
            .filter_map(|((scope, name), (function_scope, _))| {
                function_ids
                    .get(function_scope)
                    .copied()
                    .map(|function| ((*scope, name.clone()), function))
            })
            .collect();
        let function_aliases = collector
            .function_aliases
            .into_iter()
            .filter_map(|((scope, name), function_scope)| {
                function_ids
                    .get(&function_scope)
                    .copied()
                    .map(|function| ((scope, name), function))
            })
            .collect();
        let parameter_aliases = parameter_aliases_by_scope
            .into_iter()
            .filter_map(|((scope, name), provenance)| {
                function_ids
                    .get(&scope)
                    .copied()
                    .map(|function| ((function, name), provenance))
            })
            .collect();
        let collected_property_assignments = collector.property_assignments;
        let mut graph = Self {
            scopes: collector.scopes,
            scopes_by_start,
            assignments,
            binding_ids,
            function_ids,
            function_bindings,
            function_aliases,
            property_assignments: BTreeMap::new(),
            parameter_aliases,
            dynamic_evals: Vec::new(),
            mutable_static_objects: collector.mutable_static_objects.clone(),
        };
        for assignment in collected_property_assignments {
            let Some(receiver_key) = graph
                .binding_key_for_name(assignment.receiver.sym.as_ref(), assignment.receiver.span)
            else {
                continue;
            };
            graph
                .property_assignments
                .entry((
                    receiver_key,
                    assignment
                        .property
                        .strip_prefix(assignment.receiver.sym.as_ref())
                        .and_then(|path| path.strip_prefix('.'))
                        .map(|path| path.split('.').map(str::to_string).collect::<Vec<_>>())
                        .unwrap_or_default(),
                ))
                .or_default()
                .push(PropertyAliasFact {
                    span: assignment.span,
                    scope: assignment.scope,
                    target: assignment.target.map(|target| target.into()),
                });
        }
        for assignments in graph.property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        graph.dynamic_evals = collector
            .dynamic_evals
            .into_iter()
            .filter(|(_, span)| graph.binding_at("eval", *span).is_none())
            .collect();
        graph
    }

    pub(super) fn call_provenance(&self, name: &str, span: Span) -> SymbolCallProvenance {
        if self.has_dynamic_lookup_at(span) {
            return SymbolCallProvenance::Local;
        }
        match self.binding_at(name, span) {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(BindingProvenance::ValueAlias { target }) if target.is_root() => {
                SymbolCallProvenance::Global {
                    name: target.to_string(),
                }
            }
            Some(BindingProvenance::ValueAlias { target })
                if target
                    .without_bind_suffix()
                    .as_ref()
                    .is_some_and(SymbolPath::is_root) =>
            {
                SymbolCallProvenance::Global {
                    name: target
                        .without_bind_suffix()
                        .map_or_else(|| target.to_string(), |root| root.to_string()),
                }
            }
            Some(BindingProvenance::ValueAlias { target }) => self
                .module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local),
            Some(BindingProvenance::BoundCallable { target, .. }) if target.is_root() => {
                SymbolCallProvenance::Global {
                    name: target.to_string(),
                }
            }
            Some(BindingProvenance::BoundCallable { target, .. }) => self
                .module_export_for_chain(&target.to_string(), span)
                .unwrap_or(SymbolCallProvenance::Local),
            Some(BindingProvenance::BoundModuleCallable { module, export, .. }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            Some(BindingProvenance::Local | BindingProvenance::ModuleNamespace { .. })
            | Some(BindingProvenance::ReturnedObject { .. })
            | Some(
                BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => SymbolCallProvenance::Local,
            None => SymbolCallProvenance::Global {
                name: name.to_string(),
            },
        }
    }

    pub(super) fn ident_value_seed(&self, ident: &Ident) -> IdentValueSeed {
        let expr = Expr::Ident(ident.clone());
        IdentValueSeed {
            call: self.call_provenance(ident.sym.as_ref(), ident.span),
            rooted_chain: self.callable_member_chain(ident).map(Into::into),
            binding: self.binding_key_for_expr(&expr),
            constant: self.constant_value(&expr),
            bound_arguments: self.bound_arguments(ident),
        }
    }

    fn member_prop_name(&self, member: &MemberExpr) -> Option<String> {
        constant::property_name(&member.prop, self)
    }

    pub(super) fn member_chain(&self, member: &MemberExpr) -> Option<String> {
        let object = super::ast::expr_name(&member.obj)?;
        Some(format!("{object}.{}", self.member_prop_name(member)?))
    }

    pub(super) fn callable_member_chain(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ValueAlias { target } => Some(
                target
                    .without_bind_suffix()
                    .map_or_else(|| target.to_string(), |root| root.to_string()),
            ),
            BindingProvenance::BoundCallable { target, .. } => Some(target.to_string()),
            BindingProvenance::BoundModuleCallable { .. } => None,
            BindingProvenance::ReturnedObject { source } => Some(source.to_string()),
            _ => None,
        }
    }

    fn module_export_for_chain(&self, chain: &str, span: Span) -> Option<SymbolCallProvenance> {
        let (root, export) = chain.split_once('.')?;
        match self.binding_at(root, span)? {
            BindingProvenance::ModuleNamespace { module } => {
                Some(SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.to_string(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn member_call_provenance_for_chain(
        &self,
        member: &MemberExpr,
        chain: &str,
    ) -> Option<SymbolMemberProvenance> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        if let Some((module, prefix)) = self.module_member_for_expr(&member.obj) {
            let property = self.member_prop_name(member)?;
            return Some(SymbolMemberProvenance::ModuleNamespace {
                module,
                member: if prefix.is_empty() {
                    property
                } else {
                    format!("{prefix}.{property}")
                },
            });
        }
        let root = member_root_ident(member)?;
        let member = chain.strip_prefix(root.sym.as_ref())?.strip_prefix('.')?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ModuleNamespace { module }) => {
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: module.clone(),
                    member: member.to_string(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn member_value_seed(&self, member: &MemberExpr) -> MemberValueSeed {
        let syntactic_chain = self.member_chain(member).map(SymbolPath::from);
        let rooted_chain = syntactic_chain
            .as_ref()
            .and_then(|chain| self.resolve_member_chain(member, &chain.to_string()))
            .or_else(|| self.rooted_member_chain(member))
            .map(SymbolPath::from);
        let module_member = syntactic_chain
            .as_ref()
            .and_then(|chain| self.member_call_provenance_for_chain(member, &chain.to_string()));
        let returned_member = self
            .returned_member(member)
            .map(|(source, member)| (SymbolPath::from(source), member));
        MemberValueSeed {
            syntactic_chain,
            rooted_chain,
            binding: self.binding_key_for_expr(&Expr::Member(member.clone())),
            module_member,
            returned_member,
        }
    }

    fn module_member_for_expr(&self, expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ModuleExport { module, export } => {
                    Some((module.clone(), export.clone()))
                }
                BindingProvenance::ModuleNamespace { module } => {
                    Some((module.clone(), String::new()))
                }
                _ => None,
            },
            Expr::Member(member) => {
                let (module, prefix) = self.module_member_for_expr(&member.obj)?;
                let property = self.member_prop_name(member)?;
                Some((
                    module,
                    if prefix.is_empty() {
                        property
                    } else {
                        format!("{prefix}.{property}")
                    },
                ))
            }
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let Expr::Ident(require) = &**callee else {
                    return None;
                };
                if require.sym != *"require"
                    || self
                        .binding_at(require.sym.as_ref(), require.span)
                        .is_some()
                {
                    return None;
                }
                let argument = call.args.first()?;
                let Expr::Lit(swc_ecma_ast::Lit::Str(module)) = &*argument.expr else {
                    return None;
                };
                Some((module.value.to_string_lossy().to_string(), String::new()))
            }
            Expr::Paren(paren) => self.module_member_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.module_member_for_expr(expr)),
            _ => None,
        }
    }

    /// Returns the proven source call or rooted object that produced `expr`.
    /// Rooted member objects are retained as bounded provenance so callers can
    /// follow plugin instances obtained from a keyed collection without
    /// treating arbitrary `.load()`/`.unload()` spellings as APIs.
    pub(super) fn returned_object_source(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let source = self.rooted_expr_chain(callee)?;
                source.contains('.').then_some(source)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ReturnedObject { source } => Some(source.to_string()),
                _ => None,
            },
            Expr::Member(member) => {
                if let Some(source) = self.returned_object_source(&member.obj) {
                    return Some(source);
                }
                self.rooted_member_chain(member)
            }
            Expr::Paren(paren) => self.returned_object_source(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.returned_object_source(expr)),
            _ => None,
        }
    }

    pub(super) fn returned_member(&self, member: &MemberExpr) -> Option<(String, String)> {
        let source = self.returned_object_source(&member.obj)?;
        let property = self.member_prop_name(member)?;
        Some((source, property))
    }

    fn binding_at(&self, name: &str, span: Span) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
        // A declaration is the fallback. The last assignment at or before the
        // read wins, which is why assignments are sorted once during collection
        // and selected with `partition_point` here.
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .map(|index| &assignments[index].provenance)
            })
            .or_else(|| {
                self.function_ids
                    .get(&scope)
                    .and_then(|function| self.parameter_aliases.get(&(*function, name.to_string())))
            })
            .or(Some(declaration))
    }

    /// Resolve an expression to a stable lexical identity.  Semantic clients
    /// use this instead of rebuilding identity from the expression's printed
    /// member chain.
    pub(super) fn binding_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => {
                let (scope, _) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
                let binding = *self.binding_ids.get(&(scope, ident.sym.to_string()))?;
                Some(BindingKey {
                    root: BindingRoot::Binding {
                        function: self.function_scope_at(scope),
                        binding,
                        version: self.binding_version_at(scope, ident.sym.as_ref(), ident.span),
                    },
                    path: Vec::new(),
                })
            }
            Expr::Member(member) => {
                let mut key = self
                    .binding_key_for_expr(&member.obj)
                    .or_else(|| self.global_key_for_expr(&member.obj))?;
                key.path.push(self.member_prop_name(member)?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey {
                root: BindingRoot::Global("this".into()),
                path: Vec::new(),
            }),
            Expr::Paren(paren) => self.binding_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.binding_key_for_expr(expr)),
            _ => None,
        }
    }

    fn global_key_for_expr(&self, expr: &Expr) -> Option<BindingKey> {
        match expr {
            Expr::Ident(ident) => self
                .binding_at(ident.sym.as_ref(), ident.span)
                .is_none()
                .then(|| BindingKey {
                    root: BindingRoot::Global(ident.sym.to_string()),
                    path: Vec::new(),
                }),
            Expr::Member(member) => {
                let mut key = self.global_key_for_expr(&member.obj)?;
                key.path.push(self.member_prop_name(member)?);
                Some(key)
            }
            Expr::This(_) => Some(BindingKey {
                root: BindingRoot::Global("this".into()),
                path: Vec::new(),
            }),
            Expr::Paren(paren) => self.global_key_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.global_key_for_expr(expr)),
            _ => None,
        }
    }

    fn binding_version_at(&self, scope: usize, name: &str, span: Span) -> BindingVersion {
        self.assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .map(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .and_then(|index| assignments.get(index))
                    .map_or(BindingVersion(0), |assignment| assignment.version)
            })
            .unwrap_or(BindingVersion(0))
    }

    fn binding_key_for_name(&self, name: &str, span: Span) -> Option<BindingKey> {
        if let Some((scope, _)) = self.binding_with_scope_at(name, span) {
            return Some(BindingKey {
                root: BindingRoot::Binding {
                    function: self.function_scope_at(scope),
                    binding: *self.binding_ids.get(&(scope, name.to_string()))?,
                    version: self.binding_version_at(scope, name, span),
                },
                path: Vec::new(),
            });
        }
        Some(BindingKey {
            root: BindingRoot::Global(name.to_string()),
            path: Vec::new(),
        })
    }

    fn function_scope_at(&self, scope: usize) -> FunctionId {
        let mut current = Some(scope);
        while let Some(index) = current {
            if let Some(function) = self.function_ids.get(&index) {
                return *function;
            }
            current = self.scopes[index].parent;
        }
        FunctionId(0)
    }

    pub(super) fn function_id_for_scope(&self, scope: usize) -> FunctionId {
        self.function_scope_at(scope)
    }

    pub(super) fn function_id_for_expr(&self, expr: &Expr) -> Option<FunctionId> {
        let Expr::Ident(ident) = expr else {
            return None;
        };
        let (scope, provenance) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
        let function = self
            .function_bindings
            .get(&(scope, ident.sym.to_string()))
            .copied()
            .or_else(|| {
                self.function_aliases
                    .get(&(scope, ident.sym.to_string()))
                    .copied()
            })
            .or_else(|| {
                let target = match provenance {
                    BindingProvenance::ValueAlias { target }
                    | BindingProvenance::BoundCallable { target, .. } => target
                        .without_bind_suffix()
                        .unwrap_or_else(|| target.clone()),
                    _ => return None,
                };
                target
                    .is_root()
                    .then(|| self.function_binding_at(target.to_string().as_str(), ident.span))
                    .flatten()
            })?;
        let function_end = self.function_ids.iter().find_map(|(scope, candidate)| {
            (*candidate == function).then_some(self.scopes[*scope].span.hi)
        })?;
        let reassigned = self
            .assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(ident.sym.as_ref()))
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span.lo > function_end && assignment.span.lo <= ident.span.lo
                })
            });
        (!reassigned).then_some(function)
    }

    fn function_binding_at(&self, name: &str, span: Span) -> Option<FunctionId> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(function) = self.function_bindings.get(&(scope, name.to_string())) {
                return Some(*function);
            }
            scope = self.scopes.get(scope)?.parent?;
        }
    }

    pub(super) fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
        rooted_expr_chain_with(self, expr)
    }

    fn binding_with_scope_at(&self, name: &str, span: Span) -> Option<(usize, &BindingProvenance)> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(binding) = self.scopes[scope].bindings.get(name) {
                return Some((scope, binding));
            }
            scope = self.scopes[scope].parent?;
        }
    }

    fn has_dynamic_lookup_at(&self, span: Span) -> bool {
        let scope = self.scope_at(span);
        self.scope_or_ancestor_has_kind(scope, ScopeKind::Dynamic)
            || self.dynamic_evals.iter().any(|(eval_scope, eval_span)| {
                span.lo > eval_span.hi && self.scope_is_within(scope, *eval_scope)
            })
    }

    fn scope_or_ancestor_has_kind(&self, mut scope: usize, kind: ScopeKind) -> bool {
        loop {
            if self.scopes[scope].kind == kind {
                return true;
            }
            let Some(parent) = self.scopes[scope].parent else {
                return false;
            };
            scope = parent;
        }
    }

    fn scope_is_within(&self, mut scope: usize, ancestor: usize) -> bool {
        loop {
            if scope == ancestor {
                return true;
            }
            let Some(parent) = self.scopes[scope].parent else {
                return false;
            };
            scope = parent;
        }
    }

    pub(super) fn scope_at(&self, span: Span) -> usize {
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[*index].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return 0;
        };

        // Source ranges can overlap in non-nesting ways for synthetic nodes;
        // walking parents makes containment, rather than start position alone,
        // the final authority.
        while !contains(self.scopes[scope].span, span) {
            let Some(parent) = self.scopes[scope].parent else {
                return 0;
            };
            scope = parent;
        }
        scope
    }

    pub(super) fn bound_arguments(&self, ident: &Ident) -> Option<Vec<Option<BoundArgument>>> {
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::BoundCallable {
                bound_arguments, ..
            } => Some(bound_arguments.clone()),
            BindingProvenance::BoundModuleCallable {
                bound_arguments, ..
            } => Some(bound_arguments.clone()),
            _ => None,
        }
    }

    pub(super) fn scope_chain_at(&self, span: Span) -> Vec<usize> {
        let mut scopes = Vec::new();
        let mut scope = self.scope_at(span);
        loop {
            scopes.push(scope);
            let Some(parent) = self.scopes[scope].parent else {
                break;
            };
            scope = parent;
        }
        scopes
    }

    pub(super) fn unshadowed_global_at(&self, name: &str, span: Span) -> bool {
        !self.has_dynamic_lookup_at(span) && self.binding_at(name, span).is_none()
    }

    pub(super) fn mutable_static_object_at(&self, expr: &Expr) -> bool {
        let Expr::Ident(ident) = expr else {
            return false;
        };
        self.binding_with_scope_at(ident.sym.as_ref(), ident.span)
            .is_some_and(|(scope, _)| {
                self.mutable_static_objects
                    .contains(&(scope, ident.sym.to_string()))
            })
    }

    /// Evaluate constants while the lexical collector is still the source of
    /// binding facts. The resolver interns this result during its immutable
    /// build, so matcher queries do not call back into scope provenance.
    pub(super) fn constant_value(&self, expr: &Expr) -> ConstValue {
        if self.has_dynamic_lookup_at(expr.span()) {
            return ConstValue::Unknown;
        }
        constant::evaluate(expr, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_ecma_visit::{Visit, VisitWith};

    #[derive(Default)]
    struct IdentCollector {
        values: Vec<Ident>,
    }

    impl Visit for IdentCollector {
        fn visit_ident(&mut self, ident: &Ident) {
            if ident.sym == *"value" {
                self.values.push(ident.clone());
            }
        }
    }

    #[test]
    fn binding_keys_change_at_assignment_versions() {
        let parsed = crate::parse(
            "let value = source; value = replacement; use(value);",
            "bindings.js",
        )
        .expect("source should parse");
        let graph = ScopeGraph::collect(&parsed.program);
        let mut collector = IdentCollector::default();
        parsed.program.visit_with(&mut collector);
        collector.values.sort_by_key(|ident| ident.span.lo);
        let keys = collector
            .values
            .iter()
            .map(|ident| graph.binding_key_for_expr(&Expr::Ident(ident.clone())))
            .collect::<Vec<_>>();
        assert!(keys.iter().all(Option::is_some));
        assert_ne!(keys[0], keys[1]);
        assert_eq!(keys[1], keys[2]);
    }
}
