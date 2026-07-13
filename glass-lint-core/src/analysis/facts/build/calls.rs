use super::{
    BoundArgument, CallArgInfo, CallExpr, CallUnwrap, Expr, ExprOrSpread, FactBuilder, FactKind,
    FactPayload, MemberExpr, OptChainBase, ParameterBinding, Pat, PathId, PathSegment, Span,
    Spanned, SymbolCallProvenance, SymbolMemberProvenance, ValueId, ValueProjection, VisitWith,
    member_prop_name,
};

impl FactBuilder<'_> {
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
                            },
                            |argument| self.bound_arg_info(argument),
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

    pub(super) fn call_result(&mut self, span: Span) -> ValueId {
        if let Some(value) = self.call_results.get(&(span.lo.0, span.hi.0)) {
            return *value;
        }
        let value = self.resolver.fresh_object_value_at(span).id;
        self.call_results.insert((span.lo.0, span.hi.0), value);
        value
    }

    pub(super) fn value_for_expr(&mut self, expr: &Expr) -> ValueId {
        if let Expr::Call(call) = expr {
            return self.call_result(call.span());
        }
        self.resolver.resolve_expr(expr).id
    }

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
                    let path = self.append_path(path, PathSegment::Index(index as u32));
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
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        match property.as_str() {
            "call" if !call.args.is_empty() => {
                let chain = self.resolve_target_chain(&member.obj);
                let receiver = self.receiver_chain(&call.args[0].expr);
                let effective_args: Vec<_> = call.args[1..]
                    .iter()
                    .map(|a| self.arg_info(&a.expr))
                    .collect();
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let resolved = self.resolve_call_callee(target);
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    receiver,
                    effective_args,
                }));
                self.emit_call(call.span, resolved, &call.args, unwrap);
            }
            "apply" if call.args.len() >= 2 => {
                let effective_args = self.try_unwrap_apply_args(&call.args[1].expr);
                let Some(effective_args) = effective_args else {
                    return;
                };
                let chain = self.resolve_target_chain(&member.obj);
                let receiver = self.receiver_chain(&call.args[0].expr);
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let resolved = self.resolve_call_callee(target);
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    receiver,
                    effective_args,
                }));
                self.emit_call(call.span, resolved, &call.args, unwrap);
            }
            _ => {}
        }
    }

    pub(super) fn try_emit_callable_wrapper_opt(
        &mut self,
        member: &MemberExpr,
        call: &swc_ecma_ast::OptCall,
    ) {
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        match property.as_str() {
            "call" if !call.args.is_empty() => {
                let chain = self.resolve_target_chain(&member.obj);
                let receiver = self.receiver_chain(&call.args[0].expr);
                let effective_args: Vec<_> = call.args[1..]
                    .iter()
                    .map(|a| self.arg_info(&a.expr))
                    .collect();
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let resolved = self.resolve_call_callee(target);
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    receiver,
                    effective_args,
                }));
                self.emit_call(call.span, resolved, &call.args, unwrap);
            }
            "apply" if call.args.len() >= 2 => {
                let effective_args = self.try_unwrap_apply_args(&call.args[1].expr);
                let Some(effective_args) = effective_args else {
                    return;
                };
                let chain = self.resolve_target_chain(&member.obj);
                let receiver = self.receiver_chain(&call.args[0].expr);
                let target = crate::analysis::syntax::effective_callee_expr(&member.obj);
                let resolved = self.resolve_call_callee(target);
                let unwrap = Some(Box::new(CallUnwrap {
                    chain: chain.unwrap_or_default(),
                    receiver,
                    effective_args,
                }));
                self.emit_call(call.span, resolved, &call.args, unwrap);
            }
            _ => {}
        }
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

    pub(super) fn resolve_call_callee(&self, callee: &Expr) -> ResolvedCallee {
        use crate::analysis::syntax::effective_callee_expr;
        let effective = effective_callee_expr(callee);
        match effective {
            Expr::Ident(ident) => {
                let resolved = self.resolver.resolve_ident(ident);
                ResolvedCallee {
                    value: resolved.id,
                    receiver: None,
                    callee_span: ident.span,
                    callee_name: Some(ident.sym.to_string()),
                    call_provenance: resolved.call.clone(),
                    syntactic_chain: None,
                    rooted_chain: resolved.rooted_chain.clone(),
                    module_member: resolved.module_member.clone(),
                    returned_member: resolved.returned_member.clone(),
                    bound_arguments: resolved.bound_arguments.clone(),
                    instance_class: None,
                    target_function: self.resolver.function_id_for_expr(effective),
                }
            }
            Expr::Member(member) => self.resolve_member_callee(member),
            Expr::OptChain(chain) => {
                if let OptChainBase::Member(member) = &*chain.base {
                    self.resolve_member_callee(member)
                } else {
                    let resolved = self.resolver.resolve_expr(effective);
                    ResolvedCallee {
                        value: resolved.id,
                        receiver: None,
                        callee_span: effective.span(),
                        callee_name: None,
                        call_provenance: resolved.call.clone(),
                        syntactic_chain: None,
                        rooted_chain: resolved.rooted_chain.clone(),
                        module_member: resolved.module_member.clone(),
                        returned_member: resolved.returned_member.clone(),
                        bound_arguments: resolved.bound_arguments.clone(),
                        instance_class: None,
                        target_function: self.resolver.function_id_for_expr(effective),
                    }
                }
            }
            _ => {
                let resolved = self.resolver.resolve_expr(effective);
                ResolvedCallee {
                    value: resolved.id,
                    receiver: None,
                    callee_span: effective.span(),
                    callee_name: None,
                    call_provenance: resolved.call.clone(),
                    syntactic_chain: None,
                    rooted_chain: resolved.rooted_chain.clone(),
                    module_member: resolved.module_member.clone(),
                    returned_member: resolved.returned_member.clone(),
                    bound_arguments: resolved.bound_arguments.clone(),
                    instance_class: None,
                    target_function: self.resolver.function_id_for_expr(effective),
                }
            }
        }
    }

    pub(super) fn resolve_member_callee(&self, member: &MemberExpr) -> ResolvedCallee {
        let resolved = self.resolver.resolve_member(member);
        let syntactic_chain = self.resolver.member_chain(member);
        let instance_class = self.instance_class_for_receiver(&member.obj);
        ResolvedCallee {
            value: resolved.id,
            receiver: Some(self.resolver.resolve_expr(&member.obj).id),
            callee_span: member.span,
            callee_name: None,
            call_provenance: resolved.call.clone(),
            syntactic_chain,
            rooted_chain: resolved.rooted_chain.clone(),
            module_member: resolved.module_member.clone(),
            returned_member: resolved.returned_member.clone(),
            bound_arguments: resolved.bound_arguments.clone(),
            instance_class,
            target_function: self.resolver.function_id_for_expr(&member.obj),
        }
    }

    pub(super) fn instance_class_for_receiver(&self, receiver: &Expr) -> Option<(String, String)> {
        if self.static_method_depth > 0 || self.function_depth > 0 {
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

pub(super) struct ResolvedCallee {
    value: ValueId,
    receiver: Option<ValueId>,
    callee_span: Span,
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
