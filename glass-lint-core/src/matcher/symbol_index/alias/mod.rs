use std::collections::BTreeMap;

use swc_common::{Span, Spanned};
use swc_ecma_ast::{Expr, Ident, MemberExpr, OptChainBase, Program};
use swc_ecma_visit::VisitWith;

use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, member_chain, member_root_ident, object_keys,
};
use collector::AliasCollector;
use collector_helpers::{contains, member_prefix_ends};

mod collector;
mod collector_helpers;

#[derive(Debug, Default, Clone)]
pub struct AliasInfo {
    scopes: Vec<AliasScope>,
    scopes_by_start: Vec<usize>,
    assignments: BTreeMap<usize, BTreeMap<String, Vec<AliasAssignment>>>,
    property_assignments: BTreeMap<String, Vec<PropertyAliasAssignment>>,
    static_property_writes: BTreeMap<(String, String), Vec<StaticPropertyWrite>>,
    parameter_aliases: BTreeMap<(usize, String), BindingProvenance>,
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
}

#[derive(Debug, Clone)]
enum BindingProvenance {
    Local,
    ValueAlias { target: String },
    ModuleExport { module: String, export: String },
    ModuleNamespace { module: String },
    StaticString(String),
    StaticObjectKeys(Vec<String>),
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
    target: Option<String>,
}

#[derive(Debug, Clone)]
struct StaticPropertyWrite {
    span: Span,
    scope: usize,
    value: String,
}

impl AliasInfo {
    pub fn collect(program: &Program) -> Self {
        let mut collector = AliasCollector::new(program.span());
        program.visit_children_with(&mut collector);
        let parameter_aliases = collector.parameter_aliases();
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
        let mut static_property_writes =
            BTreeMap::<(String, String), Vec<StaticPropertyWrite>>::new();
        for write in collector.static_property_writes {
            static_property_writes
                .entry((write.object, write.property))
                .or_default()
                .push(StaticPropertyWrite {
                    span: write.span,
                    scope: write.scope,
                    value: write.value,
                });
        }
        for writes in static_property_writes.values_mut() {
            writes.sort_by_key(|write| write.span.lo);
        }
        Self {
            scopes: collector.scopes,
            scopes_by_start,
            assignments,
            property_assignments,
            static_property_writes,
            parameter_aliases,
        }
    }

    pub fn call_provenance(&self, name: &str, span: Span) -> SymbolCallProvenance {
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
            Some(
                BindingProvenance::Local
                | BindingProvenance::ValueAlias { .. }
                | BindingProvenance::ModuleNamespace { .. },
            )
            | Some(BindingProvenance::StaticString(_) | BindingProvenance::StaticObjectKeys(_)) => {
                SymbolCallProvenance::Local
            }
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
                _ => None,
            },
            Expr::Paren(paren) => self.object_keys_expr(&paren.expr),
            _ => None,
        }
    }

    fn static_string_at(&self, ident: &Ident) -> Option<String> {
        match self.binding_at(ident.sym.as_ref(), ident.span)? {
            BindingProvenance::StaticString(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn member_prop_name(&self, member: &MemberExpr) -> Option<String> {
        match &member.prop {
            swc_ecma_ast::MemberProp::Ident(ident) => Some(ident.sym.to_string()),
            swc_ecma_ast::MemberProp::PrivateName(name) => Some(format!("#{}", name.name)),
            swc_ecma_ast::MemberProp::Computed(computed) => {
                super::ast::static_string(&computed.expr).or_else(|| match &*computed.expr {
                    Expr::Ident(ident) => self.static_string_at(ident),
                    Expr::Paren(paren) => match &*paren.expr {
                        Expr::Ident(ident) => self.static_string_at(ident),
                        _ => None,
                    },
                    _ => None,
                })
            }
        }
    }

    pub fn member_call_provenance(&self, member: &MemberExpr) -> Option<SymbolMemberProvenance> {
        let chain = member_chain(member)?;
        self.member_call_provenance_for_chain(member, &chain)
    }

    pub fn member_call_provenance_for_chain(
        &self,
        member: &MemberExpr,
        chain: &str,
    ) -> Option<SymbolMemberProvenance> {
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

    fn binding_at(&self, name: &str, span: Span) -> Option<&BindingProvenance> {
        let (scope, declaration) = self.binding_with_scope_at(name, span)?;
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
        let syntactic_chain = member_chain(member).or_else(|| {
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
        for prefix_end in member_prefix_ends(syntactic_chain) {
            let property = &syntactic_chain[..prefix_end];
            let Some(assignments) = self.property_assignments.get(property) else {
                continue;
            };
            let prior_count =
                assignments.partition_point(|assignment| assignment.span.lo <= member.span.lo);
            if let Some(assignment) = assignments[..prior_count]
                .iter()
                .rev()
                .find(|assignment| contains(self.scopes[assignment.scope].span, member.span))
            {
                let target = assignment.target.as_ref()?;
                return Some(format!("{target}{}", &syntactic_chain[prefix_end..]));
            }
        }
        let Some(root) = member_root_ident(member) else {
            return syntactic_chain
                .starts_with("this.")
                .then(|| syntactic_chain.to_string());
        };
        let suffix = syntactic_chain.strip_prefix(root.sym.as_ref())?;
        match self.binding_at(root.sym.as_ref(), root.span) {
            Some(BindingProvenance::ValueAlias { target }) => Some(format!("{target}{suffix}")),
            Some(
                BindingProvenance::Local
                | BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. },
            )
            | Some(BindingProvenance::StaticString(_) | BindingProvenance::StaticObjectKeys(_)) => {
                None
            }
            None => Some(syntactic_chain.to_string()),
        }
    }

    pub fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
        rooted_expr_chain_with(self, expr)
    }

    pub fn has_later_static_property_write(
        &self,
        object: &str,
        property: &str,
        values: &[String],
        span: Span,
    ) -> bool {
        self.static_property_writes
            .get(&(object.to_string(), property.to_string()))
            .is_some_and(|writes| {
                writes.iter().any(|write| {
                    write.span.lo >= span.lo
                        && contains(self.scopes[write.scope].span, span)
                        && (values.is_empty() || values.iter().any(|value| value == &write.value))
                })
            })
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

    fn scope_at(&self, span: Span) -> usize {
        let position = self
            .scopes_by_start
            .partition_point(|index| self.scopes[*index].span.lo <= span.lo);
        let Some(mut scope) = position
            .checked_sub(1)
            .map(|index| self.scopes_by_start[index])
        else {
            return 0;
        };

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

impl RootedExprContext for AliasInfo {
    fn rooted_ident_chain(&self, ident: &Ident) -> Option<String> {
        match self.binding_at(ident.sym.as_ref(), ident.span) {
            Some(BindingProvenance::ValueAlias { target }) => Some(target.clone()),
            Some(_) => None,
            None => Some(ident.sym.to_string()),
        }
    }

    fn rooted_member_chain(&self, member: &MemberExpr) -> Option<String> {
        AliasInfo::rooted_member_chain(self, member)
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
