use swc_common::{Span, Spanned};
use swc_ecma_ast::{CallExpr, Callee, Expr, ExprOrSpread, OptChainBase};
use swc_ecma_visit::VisitWith;

use crate::analysis::{
    facts::{CallArgInfo, CallUnwrap, FactBuilder, FactPayload},
    syntax::{effective_callee_expr, literal_member_property_name},
    value::ValueId,
};

mod callee;
mod pattern;
mod wrapper;

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts) fn record_call_expr(&mut self, call: &CallExpr) {
        let dynamic_import = self.record_module_call_request(call);
        let Callee::Expr(callee_expr) = &call.callee else {
            let Some(callee_span) = self.byte_range(call.span) else {
                return;
            };
            let resolved = self.resolver.resolve_call_expression(call);
            let result = if matches!(call.callee, Callee::Import(_)) {
                resolved.id
            } else {
                self.call_result(call.span())
            };
            let args = self.args_info(&call.args);
            if let Some((module, span)) = dynamic_import {
                self.emit(span, FactPayload::Import { module });
            }
            self.emit(
                call.span(),
                FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    receiver: None,
                    result,
                    callee_span,
                    callee_name: None,
                    call_provenance: resolved.call,
                    syntactic_path: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                    instance_class: None,
                    target_function: None,
                    args,
                    unwrap: None,
                },
            );
            return;
        };

        if let Expr::Member(member) = effective_callee_expr(callee_expr)
            && matches!(
                literal_member_property_name(&member.prop).as_deref(),
                Some("call" | "apply")
            )
        {
            self.visit_callee_children(callee_expr);
            call.args.visit_with(self);
            self.try_emit_callable_wrapper(member, call);
            self.emit_require_import(call);
            return;
        }

        let Some(resolved) = self.resolve_call_callee(callee_expr) else {
            return;
        };
        self.visit_callee_children(callee_expr);
        call.args.visit_with(self);
        self.emit_call(call.span, resolved, &call.args, None);
        self.emit_require_import(call);
    }

    pub(in crate::analysis::facts) fn emit_call(
        &mut self,
        span: Span,
        resolved: callee::ResolvedCallee,
        args: &[ExprOrSpread],
        unwrap: Option<Box<CallUnwrap>>,
    ) {
        let result = self.call_result(span);
        let effective_args = self.effective_call_args(&resolved, args);
        let callee_name = self.intern_name(resolved.callee_name.as_deref());
        self.emit(
            span,
            FactPayload::Call {
                callee: resolved.value,
                receiver: resolved.receiver,
                result,
                callee_span: resolved.callee_span,
                callee_name,
                call_provenance: resolved.call_provenance,
                syntactic_path: resolved.syntactic_path,
                rooted_chain: self.rooted_path(resolved.rooted_chain.as_ref()),
                module_member: resolved.module_member,
                returned_member: self.returned_path(resolved.returned_member.as_ref()),
                instance_class: resolved.instance_class,
                target_function: resolved.target_function,
                args: effective_args,
                unwrap,
            },
        );
    }

    fn effective_call_args(
        &mut self,
        resolved: &callee::ResolvedCallee,
        args: &[ExprOrSpread],
    ) -> Vec<CallArgInfo> {
        let mut effective_args: Vec<CallArgInfo> = Vec::new();
        if let Some(arguments) = resolved.bound_arguments.as_deref() {
            for argument in arguments {
                effective_args.push(
                    argument
                        .as_ref()
                        .map_or_else(CallArgInfo::unknown, |arg| self.bound_arg_info(arg)),
                );
            }
        }
        effective_args.extend(self.args_info(args));
        effective_args
    }

    pub(in crate::analysis::facts) fn call_result(&mut self, span: Span) -> ValueId {
        if let Some(value) = self.call_results.get(span) {
            return value;
        }
        let value = self.resolver.fresh_object_value_at(span).id;
        self.call_results.insert(span, value);
        value
    }

    pub(in crate::analysis::facts) fn value_for_expr(&mut self, expr: &Expr) -> ValueId {
        match expr {
            Expr::Call(call) => {
                if matches!(call.callee, swc_ecma_ast::Callee::Import(_)) {
                    return self.resolver.resolve_expr_id(expr);
                }
                self.call_result(call.span())
            }
            Expr::OptChain(chain) if matches!(&*chain.base, OptChainBase::Call(_)) => {
                self.call_result(expr.span())
            }
            _ => self.resolver.resolve_expr_id(expr),
        }
    }
}
