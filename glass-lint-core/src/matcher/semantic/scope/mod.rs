//! Lexical scopes plus the narrow alias facts needed by semantic matching.
//!
//! This is not a general JavaScript interpreter. It records only stable facts
//! that can be followed without speculation: imports, unshadowed globals,
//! direct aliases, selected static shapes, and prior assignments. Unknown or
//! mutable cases intentionally resolve to local/absent provenance.

use std::collections::BTreeMap;

use swc_common::{BytePos, Span, Spanned};
use swc_ecma_ast::{Expr, Ident, MemberExpr, OptChainBase, Program};
use swc_ecma_visit::VisitWith;

use super::ast::{SymbolCallProvenance, SymbolMemberProvenance, member_root_ident, object_keys};
use collector::AliasCollector;
use collector_helpers::{contains, member_prefix_ends};

mod collector;
mod collector_helpers;

#[derive(Debug, Default, Clone)]
pub struct ScopeGraph {
    scopes: Vec<AliasScope>,
    scopes_by_start: Vec<usize>,
    assignments: BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    property_assignments: BTreeMap<String, Vec<PropertyAliasAssignment>>,
    parameter_aliases: BTreeMap<(usize, String), BindingProvenance>,
    dynamic_evals: Vec<(usize, Span)>,
}

#[derive(Debug, Clone)]
struct AliasScope {
    span: Span,
    depth: usize,
    kind: ScopeKind,
    parent: Option<usize>,
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
enum BindingProvenance {
    Local,
    ValueAlias { target: String },
    ReturnedObject { source: String },
    ModuleExport { module: String, export: String },
    ModuleNamespace { module: String },
    StaticString(String),
    StaticNumber(usize),
    StaticStringArray(Vec<String>),
    StaticObjectKeys(Vec<String>),
    StaticObjectValues(BTreeMap<String, String>),
}

#[derive(Debug, Clone)]
struct AliasAssignment {
    span: Span,
    scope: usize,
    name: String,
    provenance: BindingProvenance,
}

#[derive(Debug, Clone)]
struct PropertyAliasAssignment {
    span: Span,
    scope: usize,
    property: String,
    receiver_root: String,
    receiver_span: Span,
    target: Option<String>,
}

/// A property write is meaningful only for the exact binding (and value
/// version) of its receiver. Textual chains alone cannot distinguish a
/// parameter or block-local `table` from an outer `table`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReceiverIdentity {
    Binding {
        scope: usize,
        name: String,
        version: Option<BytePos>,
    },
    Global {
        name: String,
    },
}

impl ScopeGraph {
    pub fn collect(program: &Program) -> Self {
        let mut collector = AliasCollector::new(program.span());
        program.visit_children_with(&mut collector);
        let parameter_aliases = collector.parameter_aliases();
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
        let mut property_assignments = BTreeMap::<String, Vec<PropertyAliasAssignment>>::new();
        for assignment in collector.property_assignments {
            property_assignments
                .entry(assignment.property.clone())
                .or_default()
                .push(assignment);
        }
        for assignments in property_assignments.values_mut() {
            assignments.sort_by_key(|assignment| assignment.span.lo);
        }
        let mut graph = Self {
            scopes: collector.scopes,
            scopes_by_start,
            assignments,
            property_assignments,
            parameter_aliases,
            dynamic_evals: Vec::new(),
        };
        graph.dynamic_evals = collector
            .dynamic_evals
            .into_iter()
            .filter(|(_, span)| graph.binding_at("eval", *span).is_none())
            .collect();
        graph
    }

    pub fn call_provenance(&self, name: &str, span: Span) -> SymbolCallProvenance {
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
            Some(BindingProvenance::ValueAlias { target }) if !target.contains('.') => {
                SymbolCallProvenance::Global {
                    name: target.clone(),
                }
            }
            Some(BindingProvenance::ValueAlias { target })
                if target
                    .strip_suffix(".bind")
                    .is_some_and(|root| !root.contains('.')) =>
            {
                SymbolCallProvenance::Global {
                    name: target
                        .strip_suffix(".bind")
                        .expect("suffix was checked")
                        .to_string(),
                }
            }
            Some(BindingProvenance::ValueAlias { target }) => self
                .module_export_for_chain(target, span)
                .unwrap_or(SymbolCallProvenance::Local),
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

    pub fn object_keys_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Object(object) => object_keys(object),
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::StaticObjectKeys(keys) => Some(keys.clone()),
                BindingProvenance::StaticObjectValues(values) => {
                    Some(values.keys().cloned().collect())
                }
                _ => None,
            },
            Expr::Assign(assign) => self.object_keys_expr(&assign.right),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.object_keys_expr(expr)),
            Expr::Paren(paren) => self.object_keys_expr(&paren.expr),
            _ => None,
        }
    }

    pub fn static_string_expr(&self, expr: &Expr) -> Option<String> {
        if self.has_dynamic_lookup_at(expr.span()) {
            return None;
        }
        super::ast::static_string(expr).or_else(|| match expr {
            Expr::Ident(ident) => self.static_string_at(ident),
            Expr::Member(member) => self.static_string_member(member),
            Expr::Tpl(template) => {
                let mut value = String::new();
                for (index, quasi) in template.quasis.iter().enumerate() {
                    value.push_str(&quasi.raw);
                    if let Some(expr) = template.exprs.get(index) {
                        value.push_str(&self.static_string_expr(expr)?);
                    }
                }
                Some(value)
            }
            Expr::Bin(binary) if binary.op == swc_ecma_ast::BinaryOp::Add => Some(format!(
                "{}{}",
                self.static_string_expr(&binary.left)?,
                self.static_string_expr(&binary.right)?
            )),
            Expr::Assign(assign) => self.static_string_expr(&assign.right),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.static_string_expr(expr)),
            Expr::Paren(paren) => self.static_string_expr(&paren.expr),
            _ => None,
        })
    }

    pub(super) fn static_string_at(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::StaticString(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn static_string_member(&self, member: &MemberExpr) -> Option<String> {
        let Expr::Ident(root) = &*member.obj else {
            return None;
        };
        let index = self.member_prop_name(member)?.parse::<usize>().ok()?;
        match self.binding_at(root.sym.as_ref(), root.span)? {
            BindingProvenance::StaticStringArray(values) => values.get(index).cloned(),
            _ => None,
        }
    }

    fn member_prop_name(&self, member: &MemberExpr) -> Option<String> {
        match &member.prop {
            swc_ecma_ast::MemberProp::Ident(ident) => Some(ident.sym.to_string()),
            swc_ecma_ast::MemberProp::PrivateName(name) => Some(format!("#{}", name.name)),
            swc_ecma_ast::MemberProp::Computed(computed) => self
                .static_string_expr(&computed.expr)
                .or_else(|| {
                    self.static_number_expr(&computed.expr)
                        .map(|value| value.to_string())
                })
                .or_else(|| match &*computed.expr {
                    Expr::Ident(ident) => self.static_string_at(ident),
                    Expr::Paren(paren) => match &*paren.expr {
                        Expr::Ident(ident) => self.static_string_at(ident),
                        _ => None,
                    },
                    _ => None,
                }),
        }
    }

    pub fn member_chain(&self, member: &MemberExpr) -> Option<String> {
        let object = super::ast::expr_name(&member.obj)?;
        Some(format!("{object}.{}", self.member_prop_name(member)?))
    }

    pub fn callable_member_chain(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::ValueAlias { target } => {
                Some(target.strip_suffix(".bind").unwrap_or(target).to_string())
            }
            BindingProvenance::ReturnedObject { source } => Some(source.clone()),
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

    fn static_number_expr(&self, expr: &Expr) -> Option<usize> {
        match expr {
            Expr::Lit(swc_ecma_ast::Lit::Num(number))
                if number.value.is_finite()
                    && number.value >= 0.0
                    && number.value.fract() == 0.0 =>
            {
                Some(number.value as usize)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::StaticNumber(value) => Some(*value),
                _ => None,
            },
            Expr::Paren(paren) => self.static_number_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.static_number_expr(expr)),
            _ => None,
        }
    }

    pub fn member_call_provenance_for_chain(
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
    pub fn returned_object_source(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call(call) => {
                let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let source = self.rooted_expr_chain(callee)?;
                source.contains('.').then_some(source)
            }
            Expr::Ident(ident) => match self.binding_at(ident.sym.as_ref(), ident.span)? {
                BindingProvenance::ReturnedObject { source } => Some(source.clone()),
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

    pub fn returned_member(&self, member: &MemberExpr) -> Option<(String, String)> {
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
            .or_else(|| self.parameter_aliases.get(&(scope, name.to_string())))
            .or(Some(declaration))
    }

    pub fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        let syntactic_chain = self.member_chain(member).or_else(|| {
            let object = super::ast::expr_name(&member.obj)?;
            let property = self.member_prop_name(member)?;
            Some(format!("{object}.{property}"))
        })?;
        self.resolve_member_chain(member, &syntactic_chain)
    }

    pub fn resolve_member_chain(
        &self,
        member: &MemberExpr,
        syntactic_chain: &str,
    ) -> Option<String> {
        if self.has_dynamic_lookup_at(member.span) {
            return None;
        }
        let Some(root) = member_root_ident(member) else {
            return syntactic_chain
                .starts_with("this.")
                .then(|| syntactic_chain.to_string());
        };
        // A write to `table.api` may alias an entire prefix of a later chain
        // (`table.api.call`). Resolve the longest applicable prior write before
        // falling back to the root binding.
        for prefix_end in member_prefix_ends(syntactic_chain) {
            let property = &syntactic_chain[..prefix_end];
            let Some(assignments) = self.property_assignments.get(property) else {
                continue;
            };
            let prior_count =
                assignments.partition_point(|assignment| assignment.span.lo <= member.span.lo);
            if let Some(assignment) = assignments[..prior_count].iter().rev().find(|assignment| {
                contains(self.scopes[assignment.scope].span, member.span)
                    && self
                        .receiver_identity_at(&assignment.receiver_root, assignment.receiver_span)
                        == self.receiver_identity_at(root.sym.as_ref(), root.span)
            }) {
                let target = assignment.target.as_ref()?;
                return Some(format!("{target}{}", &syntactic_chain[prefix_end..]));
            }
        }
        let suffix = syntactic_chain.strip_prefix(root.sym.as_ref())?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ValueAlias { target }) => Some(format!("{target}{suffix}")),
            Some(BindingProvenance::ReturnedObject { source }) => Some(format!("{source}{suffix}")),
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. },
            )
            | Some(
                BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_),
            ) => None,
            None => Some(syntactic_chain.to_string()),
        }
    }

    pub fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
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

    fn receiver_identity_at(&self, name: &str, span: Span) -> ReceiverIdentity {
        let Some((scope, _)) = self.binding_with_scope_at(name, span) else {
            return ReceiverIdentity::Global {
                name: name.to_string(),
            };
        };
        let version = self
            .assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(name))
            .and_then(|assignments| {
                assignments
                    .partition_point(|assignment| assignment.span.lo <= span.lo)
                    .checked_sub(1)
                    .map(|index| assignments[index].span.lo)
            });
        ReceiverIdentity::Binding {
            scope,
            name: name.to_string(),
            version,
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
}

pub(super) trait RootedExprContext {
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<String>;
    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String>;
}

impl RootedExprContext for ScopeGraph {
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<String> {
        if self.has_dynamic_lookup_at(ident.span) {
            return None;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span) {
            Some(BindingProvenance::ValueAlias { target }) => Some(target.clone()),
            Some(BindingProvenance::ReturnedObject { source }) => Some(source.clone()),
            Some(_) => None,
            None => Some(ident.sym.to_string()),
        }
    }

    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        ScopeGraph::rooted_member_chain(self, member)
    }
}

pub(super) fn rooted_expr_chain_with(
    context: &impl RootedExprContext,
    expr: &Expr,
) -> Option<String> {
    match expr {
        Expr::This(_) => Some("this".to_string()),
        Expr::Ident(ident) => context.rooted_ident_chain(ident),
        Expr::Member(member) => context.rooted_member_chain(member),
        Expr::Call(call) => {
            let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
                return None;
            };
            rooted_expr_chain_with(context, callee)
        }
        Expr::OptChain(chain) => match &*chain.base {
            OptChainBase::Member(member) => context.rooted_member_chain(member),
            OptChainBase::Call(call) => rooted_expr_chain_with(context, &call.callee),
        },
        Expr::Paren(paren) => rooted_expr_chain_with(context, &paren.expr),
        _ => None,
    }
}
