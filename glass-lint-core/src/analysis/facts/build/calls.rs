//! Call facts and the value identities assigned to call results.
//!
//! Wrapper calls are normalized here so direct calls, `.call`, and `.apply`
//! expose one downstream representation.
//!
//! A call result is interned by source span so all roles for the same call
//! —the call fact, an assignment, and a later flow query—share one identity.

use super::{
    BoundArgument, CallArgInfo, CallExpr, CallUnwrap, Callee, Expr, ExprOrSpread, FactBuilder,
    FactKind, FactPayload, MemberExpr, OptChainBase, ParameterBinding, Pat, PathId, PathSegment,
    Span, Spanned, SymbolCallProvenance, SymbolMemberProvenance, ValueId, ValueProjection,
    VisitWith, effective_callee_expr, member_prop_name,
};

impl FactBuilder<'_> {
    /// Record a direct, imported, optional, or callable-wrapper invocation in
    /// the canonical call shape used by all matchers.
    pub(super) fn record_call_expr(&mut self, call: &CallExpr) {
        self.record_module_call_request(call);
        let Callee::Expr(callee_expr) = &call.callee else {
            let Some(callee_span) = self.byte_range(call.span) else {
                return;
            };
            let result = if matches!(call.callee, Callee::Import(_)) {
                self.resolver.resolve_expr(&Expr::Call(call.clone())).id
            } else {
                self.call_result(call.span())
            };
            let args = self.args_info(&call.args);
            self.emit(
                FactKind::Call,
                call.span(),
                FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    receiver: None,
                    result,
                    callee_span,
                    callee_name: None,
                    call_provenance: self.resolver.resolve_expr(&Expr::Call(call.clone())).call,
                    syntactic_chain: None,
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

        // Detect .call()/.apply() wrapper patterns before ordinary call
        // resolution. The wrapper fact retains the effective target and
        // arguments so all consumers agree on the same invocation shape.
        if let Expr::Member(member) = effective_callee_expr(callee_expr)
            && matches!(
                member_prop_name(&member.prop).as_deref(),
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

    /// Emit one call fact after combining wrapper-bound and source arguments.
    pub(super) fn emit_call(
        &mut self,
        span: Span,
        resolved: ResolvedCallee,
        args: &[ExprOrSpread],
        unwrap: Option<Box<CallUnwrap>>,
    ) {
        let result = self.call_result(span);
        let mut effective_args = resolved
            .bound_arguments
            .as_deref()
            .map(|arguments| {
                arguments
                    .iter()
                    .map(|argument| {
                        argument.as_ref().map_or_else(
                            || CallArgInfo {
                                value: ValueId::UNKNOWN,
                                base_value: ValueId::UNKNOWN,
                                base_path: PathId::EMPTY,
                                static_string: None,
                                object_keys: None,
                                rooted_chain: None,
                                projections: vec![ValueProjection {
                                    path: PathId::EMPTY,
                                    value: ValueId::UNKNOWN,
                                }],
                                spread: false,
                                provenance: super::SymbolCallProvenance::Local,
                            },
                            FactBuilder::bound_arg_info,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        effective_args.extend(self.args_info(args));

        self.emit(
            FactKind::Call,
            span,
            FactPayload::Call {
                callee: resolved.value,
                receiver: resolved.receiver,
                result,
                callee_span: resolved.callee_span,
                callee_name: resolved.callee_name,
                call_provenance: resolved.call_provenance,
                syntactic_chain: resolved.syntactic_chain,
                rooted_chain: resolved.rooted_chain,
                module_member: resolved.module_member,
                returned_member: resolved.returned_member,
                instance_class: resolved.instance_class,
                target_function: resolved.target_function,
                args: effective_args,
                unwrap,
            },
        );
    }

    /// Return the stable value identity representing a call's result.
    pub(super) fn call_result(&mut self, span: Span) -> ValueId {
        // Reusing the identity for a span is required: assignments and the call
        // fact must observe the same returned object.
        if let Some(value) = self.call_results.get(span) {
            return value;
        }
        let value = self.resolver.fresh_object_value_at(span).id;
        self.call_results.insert(span, value);
        value
    }

    /// Resolve the value produced by an expression, preserving call-result
    /// identity where a later declaration or assignment consumes it.
    pub(super) fn value_for_expr(&mut self, expr: &Expr) -> ValueId {
        if let Expr::Call(call) = expr {
            if matches!(call.callee, swc_ecma_ast::Callee::Import(_)) {
                return self.resolver.resolve_expr(expr).id;
            }
            return self.call_result(call.span());
        }
        self.resolver.resolve_expr(expr).id
    }

    /// Collect value identities bound by a destructuring pattern in source
    /// order; unsupported targets are deliberately omitted or unknown.
    pub(super) fn pattern_values(&self, pattern: &Pat, values: &mut Vec<ValueId>) {
        match pattern {
            Pat::Ident(ident) => values.push(self.resolver.resolve_ident(&ident.id).id),
            Pat::Assign(assign) => self.pattern_values(&assign.left, values),
            Pat::Rest(rest) => self.pattern_values(&rest.arg, values),
            Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.pattern_values(element, values);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            self.pattern_values(&property.value, values);
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            values.push(self.resolver.resolve_ident(&property.key.id).id);
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_values(&property.arg, values);
                        }
                    }
                }
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    /// Collect conservative invalidation targets for a destructuring write.
    pub(super) fn pattern_write_targets(
        &mut self,
        pattern: &Pat,
        targets: &mut Vec<(ValueId, Option<ValueId>)>,
    ) {
        match pattern {
            Pat::Ident(ident) => targets.push((self.resolver.resolve_ident(&ident.id).id, None)),
            Pat::Assign(assign) => self.pattern_write_targets(&assign.left, targets),
            Pat::Rest(rest) => self.pattern_write_targets(&rest.arg, targets),
            Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    self.pattern_write_targets(element, targets);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            self.pattern_write_targets(&property.value, targets);
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            targets.push((self.resolver.resolve_ident(&property.key.id).id, None));
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_write_targets(&property.arg, targets);
                        }
                    }
                }
            }
            Pat::Expr(expr) => {
                if let Expr::Member(member) = &**expr {
                    targets.push((
                        self.resolver.resolve_member(member).id,
                        Some(self.resolver.resolve_expr(&member.obj).id),
                    ));
                } else {
                    targets.push((self.resolver.resolve_expr(expr).id, None));
                }
            }
            Pat::Invalid(_) => {}
        }
    }

    /// Flatten a parameter pattern into path-aware bindings for interprocedural
    /// flow, retaining defaults and rest markers for downstream transfer.
    pub(super) fn parameter_bindings(
        &mut self,
        pattern: &Pat,
        parameter_index: usize,
        path: PathId,
        default: Option<ValueId>,
        rest: bool,
        output: &mut Vec<ParameterBinding>,
    ) {
        match pattern {
            Pat::Ident(ident) => output.push(ParameterBinding {
                parameter_index,
                path,
                value: self.resolver.resolve_ident(&ident.id).id,
                default,
                rest,
            }),
            Pat::Assign(assign) => {
                self.parameter_bindings(
                    &assign.left,
                    parameter_index,
                    path,
                    Some(self.resolver.resolve_expr(&assign.right).id),
                    rest,
                    output,
                );
            }
            Pat::Rest(rest_pattern) => self.parameter_bindings(
                &rest_pattern.arg,
                parameter_index,
                path,
                default,
                true,
                output,
            ),
            Pat::Array(array) => {
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else { continue };
                    let Ok(index) = u32::try_from(index) else {
                        continue;
                    };
                    let path = self.append_path(path, PathSegment::Index(index));
                    self.parameter_bindings(element, parameter_index, path, default, rest, output);
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            let Some(name) = crate::analysis::syntax::prop_name(&property.key)
                            else {
                                continue;
                            };
                            let path = self.append_path(path, PathSegment::Property(name));
                            self.parameter_bindings(
                                &property.value,
                                parameter_index,
                                path,
                                default,
                                rest,
                                output,
                            );
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            let path = self.append_path(
                                path,
                                PathSegment::Property(property.key.sym.to_string()),
                            );
                            output.push(ParameterBinding {
                                parameter_index,
                                path,
                                value: self.resolver.resolve_ident(&property.key.id).id,
                                default: property
                                    .value
                                    .as_deref()
                                    .map(|value| self.resolver.resolve_expr(value).id),
                                rest,
                            });
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            // Rest objects cannot be represented as a precise
                            // single value. Keep the binding so calls using it
                            // fail closed instead of being confused with the
                            // enclosing object.
                            self.parameter_bindings(
                                &property.arg,
                                parameter_index,
                                path,
                                default,
                                true,
                                output,
                            );
                        }
                    }
                }
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    pub(super) fn is_simple_pattern(pattern: &Pat) -> bool {
        matches!(pattern, Pat::Ident(_))
    }

    pub(super) fn try_emit_callable_wrapper(&mut self, member: &MemberExpr, call: &CallExpr) {
        self.try_emit_callable_wrapper_common(member, call.span, &call.args);
    }

    fn try_emit_callable_wrapper_common(
        &mut self,
        member: &MemberExpr,
        span: Span,
        args: &[ExprOrSpread],
    ) {
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        match property.as_str() {
            "call" if !args.is_empty() => {
                let chain = self.resolve_target_chain(&member.obj);
                let effective_args: Vec<_> =
                    args[1..].iter().map(|a| self.arg_info(&a.expr)).collect();
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let Some(resolved) = self.resolve_call_callee(target) else {
                    return;
                };
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    effective_args,
                }));
                self.emit_call(span, resolved, args, unwrap);
            }
            "apply" if args.len() >= 2 => {
                let effective_args = self.try_unwrap_apply_args(&args[1].expr);
                let Some(effective_args) = effective_args else {
                    return;
                };
                let chain = self.resolve_target_chain(&member.obj);
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let Some(resolved) = self.resolve_call_callee(target) else {
                    return;
                };
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    effective_args,
                }));
                self.emit_call(span, resolved, args, unwrap);
            }
            _ => {}
        }
    }

    pub(super) fn try_emit_callable_wrapper_opt(
        &mut self,
        member: &MemberExpr,
        call: &swc_ecma_ast::OptCall,
    ) {
        self.try_emit_callable_wrapper_common(member, call.span(), &call.args);
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
                        .map(|e| self.arg_info(&e.expr))
                        .collect(),
                )
            }
            _ => self
                .resolver
                .static_string_array_expr(args_expr)
                .map(|values| {
                    values
                        .into_iter()
                        .map(|v| CallArgInfo {
                            value: ValueId::UNKNOWN,
                            base_value: ValueId::UNKNOWN,
                            base_path: PathId::EMPTY,
                            static_string: Some(v),
                            object_keys: None,
                            rooted_chain: None,
                            projections: vec![ValueProjection {
                                path: PathId::EMPTY,
                                value: ValueId::UNKNOWN,
                            }],
                            spread: false,
                            provenance: super::SymbolCallProvenance::Local,
                        })
                        .collect()
                }),
        }
    }

    pub(super) fn emit_require_import(&mut self, call: &CallExpr) {
        if let Some(module) = self.resolver.require_module_name(call) {
            self.emit(
                FactKind::Declaration,
                call.span,
                FactPayload::Import { module },
            );
        }
    }

    pub(super) fn resolve_call_callee(&mut self, callee: &Expr) -> Option<ResolvedCallee> {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(callee);
        match effective {
            Expr::Ident(ident) => {
                let resolved = self.resolver.resolve_ident(ident);
                Some(ResolvedCallee {
                    value: resolved.id,
                    receiver: None,
                    callee_span: self.byte_range(ident.span)?,
                    callee_name: Some(ident.sym.to_string()),
                    call_provenance: resolved.call.clone(),
                    syntactic_chain: None,
                    rooted_chain: resolved.rooted_chain.clone(),
                    module_member: resolved.module_member.clone(),
                    returned_member: resolved.returned_member.clone(),
                    bound_arguments: resolved.bound_arguments,
                    instance_class: None,
                    target_function: self.resolver.function_id_for_expr(effective),
                })
            }
            Expr::Member(member) => self.resolve_member_callee(member),
            Expr::OptChain(chain) => {
                if let OptChainBase::Member(member) = &*chain.base {
                    self.resolve_member_callee(member)
                } else {
                    let resolved = self.resolver.resolve_expr(effective);
                    Some(ResolvedCallee {
                        value: resolved.id,
                        receiver: None,
                        callee_span: self.byte_range(effective.span())?,
                        callee_name: None,
                        call_provenance: resolved.call.clone(),
                        syntactic_chain: None,
                        rooted_chain: resolved.rooted_chain.clone(),
                        module_member: resolved.module_member.clone(),
                        returned_member: resolved.returned_member.clone(),
                        bound_arguments: resolved.bound_arguments,
                        instance_class: None,
                        target_function: self.resolver.function_id_for_expr(effective),
                    })
                }
            }
            _ => {
                let resolved = self.resolver.resolve_expr(effective);
                Some(ResolvedCallee {
                    value: resolved.id,
                    receiver: None,
                    callee_span: self.byte_range(effective.span())?,
                    callee_name: None,
                    call_provenance: resolved.call.clone(),
                    syntactic_chain: None,
                    rooted_chain: resolved.rooted_chain.clone(),
                    module_member: resolved.module_member.clone(),
                    returned_member: resolved.returned_member.clone(),
                    bound_arguments: resolved.bound_arguments,
                    instance_class: None,
                    target_function: self.resolver.function_id_for_expr(effective),
                })
            }
        }
    }

    pub(super) fn resolve_member_callee(&mut self, member: &MemberExpr) -> Option<ResolvedCallee> {
        let resolved = self.resolver.resolve_member(member);
        let syntactic_chain = self.resolver.member_chain(member);
        let instance_class = self.instance_class_for_receiver(&member.obj);
        Some(ResolvedCallee {
            value: resolved.id,
            receiver: Some(self.resolver.resolve_expr(&member.obj).id),
            callee_span: self.byte_range(member.span)?,
            callee_name: None,
            call_provenance: resolved.call.clone(),
            syntactic_chain,
            rooted_chain: resolved.rooted_chain.clone(),
            module_member: resolved.module_member.clone(),
            returned_member: resolved.returned_member.clone(),
            bound_arguments: resolved.bound_arguments,
            instance_class,
            target_function: self.resolver.function_id_for_expr(&member.obj),
        })
    }

    pub(super) fn instance_class_for_receiver(&self, receiver: &Expr) -> Option<(String, String)> {
        if self.traversal.in_static_method() || self.traversal.in_function() {
            return None;
        }
        let is_this = matches!(receiver, Expr::This(_))
            || matches!(receiver, Expr::Ident(ident) if ident.sym.as_ref() == "this")
            || self
                .resolver
                .resolve_expr(receiver)
                .rooted_chain
                .as_deref()
                .is_some_and(|chain| chain == "this");
        if is_this { self.current_class() } else { None }
    }

    /// Visit callee children without triggering a MemberRead fact for the
    /// callee expression itself.  The callee's semantic role is already
    /// captured in the Call fact.
    pub(super) fn visit_callee_children(&mut self, callee: &Expr) {
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

/// All resolver-backed facts needed to emit one normalized call payload.
///
/// Unknown fields remain unknown rather than being inferred from a spelling;
/// this preserves the precision boundary when a target, member, or bound
/// argument cannot be proven at the call site.
pub(super) struct ResolvedCallee {
    value: ValueId,
    receiver: Option<ValueId>,
    callee_span: crate::ByteRange,
    callee_name: Option<String>,
    call_provenance: SymbolCallProvenance,
    syntactic_chain: Option<String>,
    rooted_chain: Option<String>,
    module_member: Option<SymbolMemberProvenance>,
    returned_member: Option<(String, String)>,
    bound_arguments: Option<Vec<Option<BoundArgument>>>,
    instance_class: Option<(String, String)>,
    target_function: Option<crate::analysis::value::FunctionId>,
}
