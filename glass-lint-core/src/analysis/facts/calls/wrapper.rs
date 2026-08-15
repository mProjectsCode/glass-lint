use crate::analysis::facts::{
    CallArgInfo, CallUnwrap, Expr, ExprOrSpread, FactBuilder, MemberExpr, Span,
    effective_callee_expr, literal_member_property_name,
};

impl FactBuilder<'_, '_> {
    pub(super) fn try_emit_callable_wrapper_common(
        &mut self,
        member: &MemberExpr,
        span: Span,
        args: &[ExprOrSpread],
    ) {
        let Some(property) = literal_member_property_name(&member.prop) else {
            return;
        };
        match property.as_str() {
            "call" if !args.is_empty() => {
                let effective_args: Vec<_> = args[1..]
                    .iter()
                    .map(|a| self.arg_info_projection(&a.expr))
                    .collect();
                self.emit_callable_unwrap(member, span, args, effective_args);
            }
            "apply" if args.len() >= 2 => {
                let Some(effective_args) = self.try_unwrap_apply_args(&args[1].expr) else {
                    return;
                };
                self.emit_callable_unwrap(member, span, args, effective_args);
            }
            _ => {}
        }
    }

    /// Resolve the shared call-unwrap chain and emit the unwrapped call.
    fn emit_callable_unwrap(
        &mut self,
        member: &MemberExpr,
        span: Span,
        args: &[ExprOrSpread],
        effective_args: Vec<CallArgInfo>,
    ) {
        let chain = self.resolve_target_chain(&member.obj);
        let target = effective_callee_expr(&member.obj);
        let Some(resolved) = self.resolve_call_callee(target) else {
            return;
        };
        let chain = chain.unwrap_or_default().without_this_prefix();
        let chain_path = self.name_path(&chain);
        let unwrap = Some(Box::new(CallUnwrap {
            chain_path,
            effective_args,
        }));
        self.emit_call(span, resolved, args, unwrap);
    }

    pub(super) fn try_unwrap_apply_args(&mut self, args_expr: &Expr) -> Option<Vec<CallArgInfo>> {
        match args_expr {
            Expr::Array(array) => {
                if array
                    .elems
                    .iter()
                    .any(|e| e.as_ref().is_none_or(|e| e.spread.is_some()))
                {
                    return None;
                }
                Some(
                    array
                        .elems
                        .iter()
                        .flatten()
                        .map(|e| self.arg_info_projection(&e.expr))
                        .collect(),
                )
            }
            _ => self
                .resolver
                .static_string_array_expr(args_expr)
                .map(|values| {
                    values
                        .into_iter()
                        .map(|s| {
                            let resolved = self.resolver.static_string(s);
                            CallArgInfo {
                                value: resolved.id,
                                ..CallArgInfo::unknown()
                            }
                        })
                        .collect()
                }),
        }
    }
}
