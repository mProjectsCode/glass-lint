use glass_lint_datastructures::{NamePath, SymbolPath};
use smol_str::{SmolStr, ToSmolStr};
use swc_common::Spanned;

use crate::analysis::{
    facts::{
        Callee, Expr, FactBuilder, InstanceCallable, MemberExpr, OptChainBase,
        SymbolCallProvenance, SymbolMemberProvenance, ValueId, VisitWith,
    },
    syntax::{effective_callee_expr, member_property_name},
    value::FunctionId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::facts) struct ResolvedCallee {
    pub(in crate::analysis::facts) value: ValueId,
    pub(in crate::analysis::facts) receiver: Option<ValueId>,
    pub(in crate::analysis::facts) callee_span: glass_lint_datastructures::ByteRange,
    pub(in crate::analysis::facts) callee_name: Option<SmolStr>,
    pub(in crate::analysis::facts) call_provenance: SymbolCallProvenance,
    pub(in crate::analysis::facts) syntactic_path: Option<NamePath>,
    pub(in crate::analysis::facts) rooted_chain: Option<SymbolPath>,
    pub(in crate::analysis::facts) module_member: Option<SymbolMemberProvenance>,
    pub(in crate::analysis::facts) returned_member: Option<(SymbolPath, SymbolPath)>,
    pub(in crate::analysis::facts) bound_arguments:
        Option<Vec<Option<crate::analysis::scope::BoundArgument>>>,
    pub(in crate::analysis::facts) instance_class: Option<(SmolStr, SmolStr)>,
    pub(in crate::analysis::facts) target_function: Option<FunctionId>,
}

impl ResolvedCallee {
    pub(super) fn from_resolved(
        resolved: &std::sync::Arc<crate::analysis::resolution::ResolvedValue>,
        callee_span: glass_lint_datastructures::ByteRange,
        target_function: Option<FunctionId>,
    ) -> Self {
        Self {
            value: resolved.id,
            receiver: None,
            callee_span,
            callee_name: None,
            call_provenance: resolved.call.clone(),
            syntactic_path: None,
            rooted_chain: resolved.rooted_chain.clone(),
            module_member: resolved.module_member.clone(),
            returned_member: resolved.returned_member.clone(),
            bound_arguments: resolved.bound_arguments.clone(),
            instance_class: None,
            target_function,
        }
    }
}

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts) fn resolve_call_callee(
        &mut self,
        callee: &Expr,
    ) -> Option<ResolvedCallee> {
        let effective = effective_callee_expr(callee);
        match effective {
            Expr::Ident(ident) => {
                let resolved = self.resolver.resolve_ident(ident);
                let extracted = self.extracted_instance_callable(resolved.id);
                let instance_class = extracted.as_ref().map(InstanceCallable::class_identity);
                let syntactic_path = extracted
                    .as_ref()
                    .and_then(|callable| self.name_path(callable.member()));
                let callee_span = self.byte_range(ident.span)?;
                let callee_name = Some(ident.sym.to_smolstr());
                let target_function = self.resolver.function_id_for_expr(effective);
                let mut callee =
                    ResolvedCallee::from_resolved(&resolved, callee_span, target_function);
                callee.callee_name = callee_name;
                callee.syntactic_path = syntactic_path;
                callee.instance_class = instance_class;
                Some(callee)
            }
            Expr::Member(member) => self.resolve_member_callee(member),
            Expr::OptChain(chain) => {
                if let OptChainBase::Member(member) = &*chain.base {
                    return self.resolve_member_callee(member);
                }
                let resolved = self.resolver.resolve_expr(effective);
                let callee_span = self.byte_range(effective.span())?;
                let target_function = self.resolver.function_id_for_expr(effective);
                Some(ResolvedCallee::from_resolved(
                    &resolved,
                    callee_span,
                    target_function,
                ))
            }
            _ => {
                let resolved = self.resolver.resolve_expr(effective);
                let callee_span = self.byte_range(effective.span())?;
                let target_function = self.resolver.function_id_for_expr(effective);
                Some(ResolvedCallee::from_resolved(
                    &resolved,
                    callee_span,
                    target_function,
                ))
            }
        }
    }

    pub(in crate::analysis::facts) fn resolve_member_callee(
        &mut self,
        member: &MemberExpr,
    ) -> Option<ResolvedCallee> {
        let resolved = self.resolver.resolve_member(member);
        let syntactic_path = self
            .resolver
            .member_expression_chain(member)
            .and_then(|chain| self.name_path(&chain));
        let receiver_origin = self.instance_origin_for_expr(&member.obj);
        let instance_class = receiver_origin.or_else(|| {
            self.resolver
                .instance_member_available(member)
                .then(|| self.instance_class_for_receiver(&member.obj))
                .flatten()
        });
        let syntactic_path = syntactic_path.or_else(|| {
            instance_class.as_ref().and_then(|_| {
                member_property_name(&member.prop)
                    .and_then(|property| self.name_path(&property.into()))
            })
        });
        let receiver = Some(self.resolver.resolve_expr_id(&member.obj));
        let callee_span = self.byte_range(member.span)?;
        let target_function = self.resolver.function_id_for_expr(&member.obj);
        let mut callee = ResolvedCallee::from_resolved(&resolved, callee_span, target_function);
        callee.syntactic_path = syntactic_path;
        callee.receiver = receiver;
        callee.instance_class = instance_class;
        Some(callee)
    }

    pub(in crate::analysis::facts) fn instance_class_for_receiver(
        &mut self,
        receiver: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        if self.traversal.in_static_method() {
            return None;
        }
        if let Some(origin) = self.instance_origin_for_expr(receiver) {
            return Some(origin);
        }
        if self.traversal.in_function() {
            return None;
        }
        let is_this = matches!(receiver, Expr::This(_))
            || matches!(receiver, Expr::Ident(ident) if ident.sym.as_ref() == "this")
            || self
                .resolver
                .resolve_expr(receiver)
                .rooted_chain
                .as_ref()
                .is_some_and(|chain| chain.eq_chain("this"));
        if is_this { self.current_class() } else { None }
    }

    /// Resolve a receiver through the bounded constructed-value map. The
    /// constructor expression is resolved lazily because a member call asks
    /// about its receiver before the visitor descends into that receiver.
    pub(in crate::analysis::facts) fn instance_origin_for_expr(
        &mut self,
        expr: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        match expr {
            Expr::New(new_expr) => {
                let value = self.resolver.resolve_expr_id(expr);
                if let Some(origin) = self.instance_origins.get(value).cloned() {
                    return Some(origin);
                }
                let origin = self.instance_origin_for_constructor(&new_expr.callee)?;
                self.instance_origins
                    .insert(value, origin.clone(), self.resolver.budget);
                Some(origin)
            }
            Expr::Ident(ident) => {
                let value = self.resolver.resolve_ident_id(ident);
                self.instance_origins
                    .get(value)
                    .cloned()
                    .or_else(|| self.resolver.constructed_instance_provenance(ident))
            }
            Expr::Paren(paren) => self.instance_origin_for_expr(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.instance_origin_for_expr(expr)),
            Expr::TsAs(value) => self.instance_origin_for_expr(&value.expr),
            Expr::TsNonNull(value) => self.instance_origin_for_expr(&value.expr),
            Expr::TsSatisfies(value) => self.instance_origin_for_expr(&value.expr),
            Expr::TsTypeAssertion(value) => self.instance_origin_for_expr(&value.expr),
            _ => None,
        }
    }

    pub(in crate::analysis::facts) fn instance_origin_for_constructor(
        &mut self,
        constructor: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        self.constructor_origin_for_expr(constructor)
    }

    pub(in crate::analysis::facts) fn constructor_origin_for_expr(
        &mut self,
        constructor: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        self.resolver.class_provenance(constructor).or_else(|| {
            let value = self.resolver.resolve_expr_id(constructor);
            self.class_origins
                .get(value)
                .cloned()
                .or_else(|| match constructor {
                    Expr::Class(class_expr) => class_expr
                        .class
                        .super_class
                        .as_deref()
                        .and_then(|expr| self.resolver.class_provenance(expr)),
                    Expr::Paren(paren) => self.constructor_origin_for_expr(&paren.expr),
                    Expr::Seq(sequence) => sequence
                        .exprs
                        .last()
                        .and_then(|expr| self.constructor_origin_for_expr(expr)),
                    Expr::TsAs(value) => self.constructor_origin_for_expr(&value.expr),
                    Expr::TsNonNull(value) => self.constructor_origin_for_expr(&value.expr),
                    Expr::TsSatisfies(value) => self.constructor_origin_for_expr(&value.expr),
                    Expr::TsTypeAssertion(value) => self.constructor_origin_for_expr(&value.expr),
                    _ => None,
                })
        })
    }

    pub(in crate::analysis::facts) fn instance_callable_for_expr(
        &mut self,
        expr: &Expr,
    ) -> Option<InstanceCallable> {
        match expr {
            Expr::Ident(ident) => {
                let value = self.resolver.resolve_ident_id(ident);
                self.extracted_instance_callable(value)
            }
            Expr::Member(member) => {
                if !self.resolver.instance_member_available(member) {
                    return None;
                }
                let (module, export) = self.instance_class_for_receiver(&member.obj)?;
                let member = member_property_name(&member.prop)?;
                Some(InstanceCallable::new(module, export, member.into()))
            }
            Expr::Call(call) => {
                let Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                let Expr::Member(bind) = &**callee else {
                    return None;
                };
                (member_property_name(&bind.prop).as_deref() == Some("bind"))
                    .then(|| call.args.first())
                    .flatten()
                    .filter(|argument| matches!(&*argument.expr, Expr::This(_)))
                    .and_then(|_| self.instance_callable_for_expr(&bind.obj))
            }
            _ => None,
        }
    }

    pub(in crate::analysis::facts) fn extracted_instance_callable(
        &self,
        value: ValueId,
    ) -> Option<InstanceCallable> {
        self.instance_callables.get(&value).cloned()
    }

    pub(in crate::analysis::facts) fn visit_callee_children(&mut self, callee: &Expr) {
        match callee {
            Expr::Ident(_) => {}
            Expr::Member(member) => {
                member.obj.visit_with(self);
                member.prop.visit_with(self);
            }
            Expr::Paren(paren) => self.visit_callee_children(&paren.expr),
            Expr::Seq(sequence) => {
                for expression in sequence
                    .exprs
                    .iter()
                    .take(sequence.exprs.len().saturating_sub(1))
                {
                    expression.visit_with(self);
                }
                if let Some(expression) = sequence.exprs.last() {
                    self.visit_callee_children(expression);
                }
            }
            Expr::OptChain(chain) => match &*chain.base {
                OptChainBase::Member(member) => {
                    member.obj.visit_with(self);
                    member.prop.visit_with(self);
                }
                OptChainBase::Call(call) => self.visit_callee_children(&call.callee),
            },
            other => other.visit_with(self),
        }
    }
}
