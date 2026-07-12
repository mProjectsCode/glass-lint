//! Constructor, class, and instance provenance facts.

use swc_common::Spanned;
use swc_ecma_ast::{Expr, MemberExpr, NewExpr};

use super::super::result::ApiMatchKind;
use super::super::rule::InstanceMemberCallMatcher;
use super::ast::{SymbolCallProvenance, SymbolMemberProvenance, member_prop_name};
use super::index::MatcherFacts;
use super::resolver::Resolver;

pub(super) struct InstanceContext<'a, 'rules> {
    pub(super) index: &'a mut MatcherFacts,
    pub(super) resolver: &'a Resolver,
    pub(super) instance_matchers: &'rules [&'rules InstanceMemberCallMatcher],
    pub(super) classes: &'rules [Option<(String, String)>],
    pub(super) ordinary_functions: usize,
    pub(super) static_methods: usize,
}

impl InstanceContext<'_, '_> {
    pub(super) fn record_instance_call(&mut self, member: &MemberExpr, span: swc_common::Span) {
        if self.ordinary_functions != 0 || self.static_methods != 0 {
            return;
        }
        let receiver_is_this = matches!(&*member.obj, Expr::This(_))
            || self
                .resolver
                .resolve_expr(&member.obj)
                .rooted_chain
                .as_deref()
                .is_some_and(|chain| chain == "this");
        if !receiver_is_this {
            return;
        }
        let Some((module, export)) = self.classes.last().and_then(Option::as_ref).cloned() else {
            return;
        };
        let Some(member_name) = member_prop_name(&member.prop) else {
            return;
        };
        if self.instance_matchers.iter().any(|matcher| {
            matcher.module == module && matcher.export == export && matcher.member == member_name
        }) {
            self.index
                .instance_member_calls
                .push((module, export, member_name), span);
        }
    }
}

pub(super) fn record_module_class_expr(index: &mut MatcherFacts, resolver: &Resolver, expr: &Expr) {
    match expr {
        Expr::Ident(ident) => {
            if let SymbolCallProvenance::ModuleExport { module, export } =
                resolver.resolve_ident(ident).call
            {
                index.record(ApiMatchKind::Class, export.clone(), ident.span);
                index
                    .module_classes
                    .entry((module.clone(), export.clone()))
                    .or_default()
                    .push(ident.span);
            }
        }
        Expr::Member(member) => {
            if let Some(SymbolMemberProvenance::ModuleNamespace {
                module,
                member: member_name,
            }) = resolver.resolve_member(member).module_member
            {
                index.record(ApiMatchKind::Class, member_name.clone(), expr.span());
                index
                    .module_classes
                    .entry((module, member_name.clone()))
                    .or_default()
                    .push(expr.span());
            }
        }
        Expr::Paren(paren) => record_module_class_expr(index, resolver, &paren.expr),
        _ => {}
    }
}

pub(super) fn record_new_expr(index: &mut MatcherFacts, resolver: &Resolver, new_expr: &NewExpr) {
    let callee = super::ast::effective_callee_expr(&new_expr.callee);
    match callee {
        Expr::Ident(ident) => match resolver.resolve_ident(ident) {
            resolved
                if matches!(resolved.call, SymbolCallProvenance::Local)
                    && resolved
                        .rooted_chain
                        .as_deref()
                        .is_some_and(|name| !name.contains('.')) =>
            {
                let Some(name) = resolved.rooted_chain else {
                    return;
                };
                index.record(ApiMatchKind::Constructor, name.clone(), ident.span);
                index
                    .global_constructors
                    .entry(name)
                    .or_default()
                    .push(ident.span);
            }
            resolved => match resolved.call {
                SymbolCallProvenance::Global { name } => {
                    index.record(ApiMatchKind::Constructor, name.clone(), ident.span);
                    index
                        .global_constructors
                        .entry(name)
                        .or_default()
                        .push(ident.span);
                }
                SymbolCallProvenance::ModuleExport { module, export } => {
                    index.record(ApiMatchKind::Constructor, export.clone(), ident.span);
                    index
                        .module_constructors
                        .entry((module.clone(), export.clone()))
                        .or_default()
                        .push(ident.span);
                }
                SymbolCallProvenance::Local => {}
            },
        },
        Expr::Member(member) => {
            let resolved = resolver.resolve_member(member);
            let global_name = resolved.rooted_chain.as_deref().and_then(|chain| {
                chain
                    .strip_prefix("globalThis.")
                    .filter(|_| {
                        matches!(
                            resolver.resolve_expr(&member.obj).call,
                            SymbolCallProvenance::Global { ref name } if name == "globalThis"
                        )
                    })
                    .or((chain == "Function").then_some(chain))
            });
            if let Some(name) = global_name {
                index.record(ApiMatchKind::Constructor, name, new_expr.callee.span());
                index
                    .global_constructors
                    .entry(name.to_string())
                    .or_default()
                    .push(new_expr.callee.span());
            } else if let Some(SymbolMemberProvenance::ModuleNamespace {
                module,
                member: member_name,
            }) = resolved.module_member
            {
                index.record(
                    ApiMatchKind::Constructor,
                    member_name.clone(),
                    new_expr.callee.span(),
                );
                index
                    .module_constructors
                    .entry((module.clone(), member_name.clone()))
                    .or_default()
                    .push(new_expr.callee.span());
            }
        }
        _ => {}
    }
}
