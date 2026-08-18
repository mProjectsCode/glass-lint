use swc_common::{Span, Spanned};
use swc_ecma_ast::{CallExpr, Callee, Expr, ExprOrSpread, OptChainBase};

use crate::analysis::{
    facts::{CallArgInfo, CallUnwrap, FactBuilder, FactPayload},
    model::{fact::CallEvent, value::ValueId},
    syntax::is_dynamic_import,
};

mod callee;
mod wrapper;

pub(in crate::analysis) use callee::ResolvedCallee;
pub(in crate::analysis::facts) use wrapper::call_apply_wrapper;

impl FactBuilder<'_, '_> {
    pub(in crate::analysis::facts) fn record_call_expr(&mut self, call: &CallExpr) {
        let module_call = self.observe_module_call(call);
        let Callee::Expr(callee_expr) = &call.callee else {
            let Some(callee_span) = self.byte_range(call.span) else {
                return;
            };
            let resolved = self.resolver.resolve_call_expression(call);
            let result = if is_dynamic_import(&call.callee) {
                resolved.id
            } else {
                self.call_result(call.span())
            };
            let args = self.args_info(&call.args);
            if let Some(module) = module_call {
                self.emit(call.span(), FactPayload::Import { module });
            }
            self.emit(
                call.span(),
                FactPayload::Call(CallEvent::unknown(
                    result,
                    callee_span,
                    resolved.provenance.call.clone(),
                    args,
                )),
            );
            return;
        };

        let wrapper = call_apply_wrapper(callee_expr);
        if let Some(module) = module_call {
            self.emit(call.span, FactPayload::Import { module });
        }
        self.record_call_like(call.span, callee_expr, &call.args, wrapper);
    }

    pub(in crate::analysis::facts) fn record_call_like(
        &mut self,
        span: Span,
        callee_expr: &Expr,
        args: &[ExprOrSpread],
        wrapper: Option<&swc_ecma_ast::MemberExpr>,
    ) {
        if let Some(member) = wrapper {
            self.visit_callee_children(callee_expr);
            self.try_emit_callable_wrapper_common(member, span, args);
            return;
        }
        let Some(resolved) = self.resolve_call_callee(callee_expr) else {
            return;
        };
        self.visit_callee_children(callee_expr);
        self.emit_call(span, resolved, args, None);
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
        let rooted_chain = resolved
            .resolved
            .provenance
            .rooted_chain
            .as_ref()
            .map(|path| self.rooted_name_path(path));
        let returned_member =
            Self::returned_path(resolved.resolved.provenance.returned_member.as_ref());
        let event = CallEvent::from_resolved(
            resolved,
            result,
            callee_name,
            rooted_chain,
            returned_member,
            effective_args,
            unwrap,
        );
        self.emit(span, FactPayload::Call(event));
    }

    fn effective_call_args(
        &mut self,
        resolved: &callee::ResolvedCallee,
        args: &[ExprOrSpread],
    ) -> Vec<CallArgInfo> {
        let mut effective_args: Vec<CallArgInfo> = Vec::new();
        if let Some(arguments) = resolved.resolved.provenance.bound_arguments.as_deref() {
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
                if is_dynamic_import(&call.callee) {
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
