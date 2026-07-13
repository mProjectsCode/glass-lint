//! The single authoritative semantic fact walk.
//!
//! `FactBuilder` is the only post-scope SWC visitor.  It resolves
//! identities, interns values, and emits one canonical `SemanticFact` for
//! each semantic role.  It does not receive matchers or populate evidence.

use std::collections::BTreeMap;

use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BinExpr, BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, CondExpr,
    DoWhileStmt, ExportDecl, Expr, ExprOrSpread, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function,
    Ident, IfStmt, ImportDecl, MemberExpr, NewExpr, OptChainBase, OptChainExpr, Pat, Str,
    SwitchStmt, Tpl, TryStmt, UnaryExpr, UnaryOp, UpdateExpr, VarDeclarator, WhileStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, member_prop_name,
};
use super::facts::{
    CallArgInfo, CallUnwrap, ControlKind, FactId, FactKind, FactPayload, FactStream,
    FunctionBoundary, ParameterBinding, ProjectionSegment, SemanticFact, ValueProjection,
};
use super::resolution::Resolver;
use super::scope::BoundArgument;
use super::value::ValueId;

/// The single authoritative semantic fact builder.  After the lexical
/// scope prepass, this visitor walks the AST exactly once and emits an
/// immutable `FactStream` containing all semantic facts.
pub(super) struct FactBuilder<'a> {
    resolver: &'a Resolver,
    stream: FactStream,
    next_id: u32,
    next_control_region: u32,
    class_stack: Vec<Option<(String, String)>>,
    function_depth: usize,
    static_method_depth: usize,
    call_results: BTreeMap<(u32, u32), ValueId>,
}

impl<'a> FactBuilder<'a> {
    pub(super) fn new(resolver: &'a Resolver) -> Self {
        Self {
            resolver,
            stream: FactStream::new(),
            next_id: 0,
            next_control_region: 0,
            class_stack: Vec::new(),
            function_depth: 0,
            static_method_depth: 0,
            call_results: BTreeMap::new(),
        }
    }

    fn next_fact_id(&mut self) -> Option<FactId> {
        if self.next_id as usize >= super::facts::MAX_FACTS {
            return None;
        }
        let id = FactId(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        Some(id)
    }

    fn scope_at(&self, span: Span) -> usize {
        self.resolver
            .scope_chain_at(span)
            .first()
            .copied()
            .unwrap_or(0)
    }

    fn emit(&mut self, kind: FactKind, span: Span, payload: FactPayload) {
        let Some(id) = self.next_fact_id() else {
            self.stream.push(SemanticFact {
                id: FactId(self.next_id),
                span,
                scope: 0,
                function: super::value::FunctionId(0),
                kind,
                payload,
            });
            return;
        };
        let scope = self.scope_at(span);
        let fact = SemanticFact {
            id,
            span,
            scope,
            function: self.resolver.function_id_for_scope(scope),
            kind,
            payload,
        };
        self.stream.push(fact);
    }

    pub(super) fn into_stream(self) -> FactStream {
        self.stream
    }

    fn current_class(&self) -> Option<(String, String)> {
        self.class_stack.last().and_then(Clone::clone)
    }

    fn arg_info(&self, expr: &Expr) -> CallArgInfo {
        let value = self.resolver.resolve_expr(expr).id;
        let (base_value, base_path) = self.expression_projection(expr);
        let mut projections = Vec::new();
        self.collect_value_projections(expr, &mut Vec::new(), &mut projections);
        if projections.is_empty() {
            projections.push(ValueProjection {
                path: Vec::new(),
                value,
            });
        }
        CallArgInfo {
            value,
            base_value,
            base_path,
            static_string: self.resolver.static_string_expr(expr),
            object_keys: self.resolver.object_keys_expr(expr),
            rooted_chain: self.resolver.rooted_expr_chain(expr),
            projections,
            spread: false,
        }
    }

    fn expression_projection(&self, expr: &Expr) -> (ValueId, Vec<ProjectionSegment>) {
        match expr {
            Expr::Member(member) => {
                let (base, mut path) = self.expression_projection(&member.obj);
                let Some(property) = member_prop_name(&member.prop) else {
                    return (ValueId::UNKNOWN, Vec::new());
                };
                if let Ok(index) = property.parse::<usize>() {
                    path.push(ProjectionSegment::Index(index));
                } else {
                    path.push(ProjectionSegment::Property(property));
                }
                (base, path)
            }
            Expr::Paren(paren) => self.expression_projection(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .map_or((self.resolver.resolve_expr(expr).id, Vec::new()), |last| {
                    self.expression_projection(last)
                }),
            _ => (self.resolver.resolve_expr(expr).id, Vec::new()),
        }
    }

    fn collect_value_projections(
        &self,
        expr: &Expr,
        path: &mut Vec<ProjectionSegment>,
        output: &mut Vec<ValueProjection>,
    ) {
        output.push(ValueProjection {
            path: path.clone(),
            value: self.resolver.resolve_expr(expr).id,
        });
        match expr {
            Expr::Object(object) => {
                for property in &object.props {
                    let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                        continue;
                    };
                    let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                        continue;
                    };
                    let Some(name) = super::ast::prop_name(&property.key) else {
                        continue;
                    };
                    path.push(ProjectionSegment::Property(name));
                    self.collect_value_projections(&property.value, path, output);
                    path.pop();
                }
            }
            Expr::Array(array) => {
                for (index, element) in array.elems.iter().enumerate() {
                    let Some(element) = element else { continue };
                    path.push(ProjectionSegment::Index(index));
                    self.collect_value_projections(&element.expr, path, output);
                    path.pop();
                }
            }
            Expr::Paren(paren) => self.collect_value_projections(&paren.expr, path, output),
            Expr::Seq(sequence) => {
                if let Some(last) = sequence.exprs.last() {
                    self.collect_value_projections(last, path, output);
                }
            }
            _ => {}
        }
    }

    fn bound_arg_info(&self, argument: &BoundArgument) -> CallArgInfo {
        match argument {
            BoundArgument::StaticString(value) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: Vec::new(),
                static_string: Some(value.clone()),
                object_keys: None,
                rooted_chain: None,
                projections: vec![ValueProjection {
                    path: Vec::new(),
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
            BoundArgument::RootedExpression(chain) => CallArgInfo {
                value: ValueId::UNKNOWN,
                base_value: ValueId::UNKNOWN,
                base_path: Vec::new(),
                static_string: None,
                object_keys: None,
                rooted_chain: Some(chain.to_string()),
                projections: vec![ValueProjection {
                    path: Vec::new(),
                    value: ValueId::UNKNOWN,
                }],
                spread: false,
            },
        }
    }

    fn args_info(&self, args: &[ExprOrSpread]) -> Vec<CallArgInfo> {
        args.iter()
            .map(|arg| {
                let mut info = self.arg_info(&arg.expr);
                info.spread = arg.spread.is_some();
                if info.spread {
                    info.projections.clear();
                }
                info
            })
            .collect()
    }

    fn resolve_target_chain(&self, target: &Expr) -> Option<String> {
        use super::ast::effective_callee_expr;
        let effective = effective_callee_expr(target);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .clone()
                .or_else(|| Some(ident.sym.to_string())),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain.clone(),
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }

    fn receiver_chain(&self, expr: &Expr) -> Option<String> {
        use super::ast::effective_callee_expr;
        let effective = effective_callee_expr(expr);
        match effective {
            Expr::Ident(ident) => self
                .resolver
                .resolve_ident(ident)
                .rooted_chain
                .clone()
                .or_else(|| Some(ident.sym.to_string())),
            Expr::Member(member) => self.resolver.resolve_member(member).rooted_chain.clone(),
            _ => self.resolver.rooted_expr_chain(effective),
        }
    }

    fn emit_call(
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
                                base_path: Vec::new(),
                                static_string: None,
                                object_keys: None,
                                rooted_chain: None,
                                projections: vec![ValueProjection {
                                    path: Vec::new(),
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

    fn call_result(&mut self, span: Span) -> ValueId {
        if let Some(value) = self.call_results.get(&(span.lo.0, span.hi.0)) {
            return *value;
        }
        let value = self.resolver.fresh_object_value_at(span).id;
        self.call_results.insert((span.lo.0, span.hi.0), value);
        value
    }

    fn value_for_expr(&mut self, expr: &Expr) -> ValueId {
        if let Expr::Call(call) = expr {
            return self.call_result(call.span());
        }
        self.resolver.resolve_expr(expr).id
    }

    fn pattern_values(&self, pattern: &Pat, values: &mut Vec<ValueId>) {
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
                            self.pattern_values(&property.value, values)
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            values.push(self.resolver.resolve_ident(&property.key.id).id)
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_values(&property.arg, values)
                        }
                    }
                }
            }
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    fn pattern_write_targets(&self, pattern: &Pat, targets: &mut Vec<(ValueId, Option<ValueId>)>) {
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
                            self.pattern_write_targets(&property.value, targets)
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            targets.push((self.resolver.resolve_ident(&property.key.id).id, None))
                        }
                        swc_ecma_ast::ObjectPatProp::Rest(property) => {
                            self.pattern_write_targets(&property.arg, targets)
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

    fn parameter_bindings(
        &self,
        pattern: &Pat,
        parameter_index: usize,
        path: &mut Vec<ProjectionSegment>,
        default: Option<ValueId>,
        rest: bool,
        output: &mut Vec<ParameterBinding>,
    ) {
        match pattern {
            Pat::Ident(ident) => output.push(ParameterBinding {
                parameter_index,
                path: path.clone(),
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
                    path.push(ProjectionSegment::Index(index));
                    self.parameter_bindings(element, parameter_index, path, default, rest, output);
                    path.pop();
                }
            }
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        swc_ecma_ast::ObjectPatProp::KeyValue(property) => {
                            let Some(name) = super::ast::prop_name(&property.key) else {
                                continue;
                            };
                            path.push(ProjectionSegment::Property(name));
                            self.parameter_bindings(
                                &property.value,
                                parameter_index,
                                path,
                                default,
                                rest,
                                output,
                            );
                            path.pop();
                        }
                        swc_ecma_ast::ObjectPatProp::Assign(property) => {
                            path.push(ProjectionSegment::Property(property.key.sym.to_string()));
                            output.push(ParameterBinding {
                                parameter_index,
                                path: path.clone(),
                                value: self.resolver.resolve_ident(&property.key.id).id,
                                default: property
                                    .value
                                    .as_deref()
                                    .map(|value| self.resolver.resolve_expr(value).id),
                                rest,
                            });
                            path.pop();
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

    fn is_simple_pattern(pattern: &Pat) -> bool {
        matches!(pattern, Pat::Ident(_))
    }

    fn next_control_region(&mut self) -> u32 {
        let region = self.next_control_region;
        self.next_control_region = self.next_control_region.saturating_add(1);
        region
    }

    fn emit_control(&mut self, span: Span, kind: ControlKind, region: u32) {
        self.emit(
            FactKind::Control,
            span,
            FactPayload::Control { kind, region },
        );
    }

    fn emit_function_fact(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = (usize, Pat)>,
        boundary: FunctionBoundary,
    ) {
        let scope = self.scope_at(span);
        let id = self.resolver.function_id_for_scope(scope);
        let owner = self
            .resolver
            .scope_chain_at(span)
            .get(1)
            .copied()
            .map_or(id, |scope| self.resolver.function_id_for_scope(scope));
        let mut parameter_bindings = Vec::new();
        for (parameter_index, parameter) in parameters {
            self.parameter_bindings(
                &parameter,
                parameter_index,
                &mut Vec::new(),
                None,
                false,
                &mut parameter_bindings,
            );
        }
        self.emit(
            FactKind::Function,
            span,
            FactPayload::Function {
                id,
                owner,
                name: None,
                parameters: parameter_bindings,
                boundary,
            },
        );
    }

    fn try_emit_callable_wrapper(&mut self, member: &MemberExpr, call: &CallExpr) {
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
                let target = super::ast::effective_callee_expr(&member.obj);
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
                let target = super::ast::effective_callee_expr(&member.obj);
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

    fn try_emit_callable_wrapper_opt(&mut self, member: &MemberExpr, call: &swc_ecma_ast::OptCall) {
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
                let target = super::ast::effective_callee_expr(&member.obj);
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
                let target = super::ast::effective_callee_expr(&member.obj);
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

    fn try_unwrap_apply_args(&self, args_expr: &Expr) -> Option<Vec<CallArgInfo>> {
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
                            base_path: Vec::new(),
                            static_string: Some(v),
                            object_keys: None,
                            rooted_chain: None,
                            projections: vec![ValueProjection {
                                path: Vec::new(),
                                value: ValueId::UNKNOWN,
                            }],
                            spread: false,
                        })
                        .collect()
                }),
        }
    }

    fn emit_require_import(&mut self, call: &CallExpr) {
        if let Some(module) = self.resolver.require_module_name(call) {
            self.emit(
                FactKind::Declaration,
                call.span,
                FactPayload::Import { module },
            );
        }
    }

    fn resolve_call_callee(&self, callee: &Expr) -> ResolvedCallee {
        use super::ast::effective_callee_expr;
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
            Expr::OptChain(chain) => match &*chain.base {
                OptChainBase::Member(member) => self.resolve_member_callee(member),
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
            },
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

    fn resolve_member_callee(&self, member: &MemberExpr) -> ResolvedCallee {
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

    fn instance_class_for_receiver(&self, receiver: &Expr) -> Option<(String, String)> {
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
    fn visit_callee_children(&mut self, callee: &Expr) {
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

struct ResolvedCallee {
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
    target_function: Option<super::value::FunctionId>,
}

#[allow(clippy::too_many_lines)]
impl Visit for FactBuilder<'_> {
    fn visit_ident(&mut self, ident: &Ident) {
        let resolved = self.resolver.resolve_ident(ident);
        self.emit(
            FactKind::Reference,
            ident.span(),
            FactPayload::Reference {
                value: resolved.id,
                static_string: None,
            },
        );
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        let resolved = self.resolver.resolve_member(member);
        let syntactic_chain = self.resolver.member_chain(member);
        self.emit(
            FactKind::MemberRead,
            member.span(),
            FactPayload::MemberRead {
                value: resolved.id,
                syntactic_chain,
                rooted_chain: resolved.rooted_chain.clone(),
                module_member: resolved.module_member.clone(),
                returned_member: resolved.returned_member.clone(),
            },
        );
        member.visit_children_with(self);
    }

    fn visit_var_declarator(&mut self, declarator: &VarDeclarator) {
        let mut source = declarator
            .init
            .as_ref()
            .map(|init| self.value_for_expr(init))
            .unwrap_or(ValueId::UNKNOWN);
        // Initializers are evaluated before the declaration becomes visible.
        // Emit the declaration after visiting the initializer so fact order is
        // an evaluation order, not merely an AST preorder.
        if let Some(init) = &declarator.init {
            init.visit_with(self);
        }
        declarator.name.visit_with(self);
        let mut targets = Vec::new();
        self.pattern_values(&declarator.name, &mut targets);
        if targets.is_empty() {
            targets.push(ValueId::UNKNOWN);
        }
        if !Self::is_simple_pattern(&declarator.name) {
            source = ValueId::UNKNOWN;
        }
        for target in targets {
            self.emit(
                FactKind::Declaration,
                declarator.span(),
                FactPayload::Declaration { target, source },
            );
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let source = self.value_for_expr(&assignment.right);
        match &assignment.left {
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(ident)) => {
                assignment.right.visit_with(self);
                let target = self.resolver.resolve_ident(&ident.id).id;
                self.emit(
                    FactKind::Assignment,
                    assignment.span(),
                    FactPayload::Assignment {
                        target,
                        source,
                        receiver: None,
                    },
                );
            }
            swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(
                member,
            )) => {
                // Evaluate the member reference (including computed keys) and
                // the RHS before emitting the write/kill fact.
                member.obj.visit_with(self);
                member.prop.visit_with(self);
                let resolved_member = self.resolver.resolve_member(member);
                self.emit(
                    FactKind::MemberRead,
                    member.span(),
                    FactPayload::MemberRead {
                        value: resolved_member.id,
                        syntactic_chain: self.resolver.member_chain(member),
                        rooted_chain: resolved_member.rooted_chain.clone(),
                        module_member: resolved_member.module_member.clone(),
                        returned_member: resolved_member.returned_member.clone(),
                    },
                );
                assignment.right.visit_with(self);
                let target = resolved_member.id;
                let property = member_prop_name(&member.prop);
                if assignment.op == swc_ecma_ast::AssignOp::Assign {
                    self.emit(
                        FactKind::PropertyWrite,
                        assignment.span(),
                        FactPayload::PropertyWrite {
                            target,
                            receiver: self.resolver.resolve_expr(&member.obj).id,
                            source,
                            property,
                            static_value: self.resolver.static_string_expr(&assignment.right),
                        },
                    );
                } else {
                    self.emit(
                        FactKind::Assignment,
                        assignment.span(),
                        FactPayload::Assignment {
                            target,
                            source: ValueId::UNKNOWN,
                            receiver: Some(self.resolver.resolve_expr(&member.obj).id),
                        },
                    );
                }
            }
            swc_ecma_ast::AssignTarget::Pat(pattern) => {
                assignment.right.visit_with(self);
                let pattern: Pat = pattern.clone().into();
                let mut targets = Vec::new();
                self.pattern_write_targets(&pattern, &mut targets);
                for (target, receiver) in targets {
                    self.emit(
                        FactKind::Assignment,
                        assignment.span(),
                        FactPayload::Assignment {
                            target,
                            source: ValueId::UNKNOWN,
                            receiver,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn visit_update_expr(&mut self, update: &UpdateExpr) {
        update.arg.visit_with(self);
        let target = self.resolver.resolve_expr(&update.arg).id;
        self.emit(
            FactKind::Assignment,
            update.span(),
            FactPayload::Assignment {
                target,
                source: ValueId::UNKNOWN,
                receiver: match &*update.arg {
                    Expr::Member(member) => Some(self.resolver.resolve_expr(&member.obj).id),
                    _ => None,
                },
            },
        );
    }

    fn visit_unary_expr(&mut self, unary: &UnaryExpr) {
        unary.arg.visit_with(self);
        if unary.op == UnaryOp::Delete {
            let target = self.resolver.resolve_expr(&unary.arg).id;
            self.emit(
                FactKind::Assignment,
                unary.span(),
                FactPayload::Assignment {
                    target,
                    source: ValueId::UNKNOWN,
                    receiver: match &*unary.arg {
                        Expr::Member(member) => Some(self.resolver.resolve_expr(&member.obj).id),
                        _ => None,
                    },
                },
            );
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        let Callee::Expr(callee_expr) = &call.callee else {
            let result = self.call_result(call.span());
            self.emit(
                FactKind::Call,
                call.span(),
                FactPayload::Call {
                    callee: ValueId::UNKNOWN,
                    receiver: None,
                    result,
                    callee_span: call.span,
                    callee_name: None,
                    call_provenance: SymbolCallProvenance::Local,
                    syntactic_chain: None,
                    rooted_chain: None,
                    module_member: None,
                    returned_member: None,
                    instance_class: None,
                    target_function: None,
                    args: self.args_info(&call.args),
                    unwrap: None,
                },
            );
            return;
        };

        // Detect .call()/.apply() wrapper pattern.
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

        let resolved = self.resolve_call_callee(callee_expr);
        self.visit_callee_children(callee_expr);
        call.args.visit_with(self);
        self.emit_call(call.span, resolved, &call.args, None);
        self.emit_require_import(call);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        match &*chain.base {
            OptChainBase::Call(call) => {
                let callee_expr = &call.callee;
                // Detect .call()/.apply() inside optional chain.
                let optional_member = match effective_callee_expr(callee_expr) {
                    Expr::Member(member) => Some(member),
                    Expr::OptChain(inner) => match &*inner.base {
                        OptChainBase::Member(member) => Some(member),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(member) = optional_member
                    && matches!(
                        member_prop_name(&member.prop).as_deref(),
                        Some("call" | "apply")
                    )
                {
                    self.visit_callee_children(callee_expr);
                    call.args.visit_with(self);
                    self.try_emit_callable_wrapper_opt(member, call);
                } else {
                    let resolved = self.resolve_call_callee(callee_expr);
                    self.visit_callee_children(callee_expr);
                    call.args.visit_with(self);
                    self.emit_call(chain.span(), resolved, &call.args, None);
                }
            }
            OptChainBase::Member(member) => {
                let resolved = self.resolver.resolve_member(member);
                let syntactic_chain = self.resolver.member_chain(member);
                self.emit(
                    FactKind::MemberRead,
                    member.span(),
                    FactPayload::MemberRead {
                        value: resolved.id,
                        syntactic_chain,
                        rooted_chain: resolved.rooted_chain.clone(),
                        module_member: resolved.module_member.clone(),
                        returned_member: resolved.returned_member.clone(),
                    },
                );
                member.visit_children_with(self);
            }
        }
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        let resolved = self.resolver.resolve_expr(&new_expr.callee);
        let callee_span = new_expr.callee.span();

        // Resolve callee name and provenance for member expression callees
        // like `new globalThis.URL(...)` or `new mod.Foo(...)`.
        let (callee_name, provenance) = match &*new_expr.callee {
            Expr::Ident(ident) => {
                let p = resolved.call.clone();
                (
                    Some(
                        resolved
                            .rooted_chain
                            .clone()
                            .unwrap_or_else(|| ident.sym.to_string()),
                    ),
                    p,
                )
            }
            Expr::Member(member) => {
                let member_resolved = self.resolver.resolve_member(member);
                let global_name = member_resolved.rooted_chain.as_deref().and_then(|chain| {
                    chain
                        .strip_prefix("globalThis.")
                        .filter(|_| {
                            matches!(
                                self.resolver.resolve_expr(&member.obj).call,
                                SymbolCallProvenance::Global { ref name } if name == "globalThis"
                            )
                        })
                        .or((chain == "Function").then_some(chain))
                });
                if let Some(name) = global_name {
                    let name = name.to_string();
                    (Some(name.clone()), SymbolCallProvenance::Global { name })
                } else if let Some(SymbolMemberProvenance::ModuleNamespace {
                    module,
                    member: member_name,
                }) = member_resolved.module_member
                {
                    (
                        Some(member_name.clone()),
                        SymbolCallProvenance::ModuleExport {
                            module,
                            export: member_name,
                        },
                    )
                } else {
                    (None, resolved.call.clone())
                }
            }
            _ => (None, resolved.call.clone()),
        };

        new_expr.visit_children_with(self);
        let result = self.resolver.fresh_object_value_at(new_expr.span).id;
        self.emit(
            FactKind::Construction,
            new_expr.span(),
            FactPayload::Construction {
                callee: resolved.id,
                result,
                callee_span,
                callee_name,
                provenance,
            },
        );
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        self.emit(
            FactKind::Declaration,
            import.src.span,
            FactPayload::Import { module },
        );
        // Do not visit children: the source string is already captured in the
        // Import fact, and visiting it would emit a duplicate static reference.
    }

    fn visit_str(&mut self, value: &Str) {
        let literal = value.value.to_string_lossy().to_string();
        self.emit(
            FactKind::Reference,
            value.span(),
            FactPayload::Reference {
                value: self
                    .resolver
                    .resolve_expr(&Expr::Lit(swc_ecma_ast::Lit::Str(value.clone())))
                    .id,
                static_string: Some(literal),
            },
        );
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        for quasi in &template.quasis {
            let literal = quasi
                .cooked
                .as_ref()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| quasi.raw.to_string());
            self.emit(
                FactKind::Reference,
                quasi.span,
                FactPayload::Reference {
                    value: ValueId::UNKNOWN,
                    static_string: Some(literal),
                },
            );
        }
        template.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        let name = class_decl.ident.sym.to_string();
        let provenance = class_decl
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.emit(
            FactKind::Declaration,
            class_decl.ident.span(),
            FactPayload::Class {
                name,
                provenance: provenance.clone(),
            },
        );
        self.class_stack.push(provenance);
        class_decl.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr) {
        let provenance = class_expr
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        if let Some(ident) = &class_expr.ident {
            self.emit(
                FactKind::Declaration,
                ident.span(),
                FactPayload::Class {
                    name: ident.sym.to_string(),
                    provenance: provenance.clone(),
                },
            );
        }
        self.class_stack.push(provenance);
        class_expr.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_bin_expr(&mut self, binary: &BinExpr) {
        if binary.op == BinaryOp::InstanceOf {
            let provenance = self.resolver.class_provenance(&binary.right);
            self.emit(
                FactKind::Reference,
                binary.right.span(),
                FactPayload::Class {
                    name: String::new(),
                    provenance,
                },
            );
        }
        binary.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, function: &FnDecl) {
        self.function_depth += 1;
        function.visit_children_with(self);
        self.function_depth -= 1;
    }

    fn visit_function(&mut self, function: &Function) {
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, p)| (index, p.pat.clone())),
            FunctionBoundary::Enter,
        );
        self.function_depth += 1;
        function.visit_children_with(self);
        self.function_depth -= 1;
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, p)| (index, p.pat.clone())),
            FunctionBoundary::Exit,
        );
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Enter,
        );
        arrow.body.visit_with(self);
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Exit,
        );
    }

    fn visit_class_method(&mut self, method: &swc_ecma_ast::ClassMethod) {
        self.emit_function_fact(
            method.function.span(),
            method
                .function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Enter,
        );
        if method.is_static {
            self.static_method_depth += 1;
        }
        // Visit only the method body so the method gets one function boundary
        // pair, rather than a nested duplicate Function walk.
        if let Some(body) = method.function.body.as_ref() {
            body.visit_with(self);
        }
        self.emit_function_fact(
            method.function.span(),
            method
                .function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Exit,
        );
        if method.is_static {
            self.static_method_depth -= 1;
        }
    }

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::BranchStart, region);
        stmt.test.visit_with(self);
        self.emit_control(stmt.cons.span(), ControlKind::BranchThen, region);
        stmt.cons.visit_with(self);
        if let Some(alt) = &stmt.alt {
            self.emit_control(alt.span(), ControlKind::BranchElse, region);
            alt.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::BranchEnd, region);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        if let Some(init) = &stmt.init {
            init.visit_with(self);
        }
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        // The test is evaluated before the first iteration. The update is
        // evaluated after the body, matching JavaScript evaluation order.
        if let Some(test) = &stmt.test {
            test.visit_with(self);
        }
        stmt.body.visit_with(self);
        if let Some(update) = &stmt.update {
            self.emit_control(stmt.span(), ControlKind::LoopUpdate, region);
            update.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_while_stmt(&mut self, stmt: &WhileStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: false },
            region,
        );
        stmt.test.visit_with(self);
        stmt.body.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_do_while_stmt(&mut self, stmt: &DoWhileStmt) {
        let region = self.next_control_region();
        self.emit_control(
            stmt.span(),
            ControlKind::LoopStart { guaranteed: true },
            region,
        );
        stmt.body.visit_with(self);
        stmt.test.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::LoopEnd, region);
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::SwitchStart, region);
        stmt.discriminant.visit_with(self);
        for case in &stmt.cases {
            self.emit_control(
                case.span(),
                ControlKind::SwitchCase {
                    is_default: case.test.is_none(),
                },
                region,
            );
            case.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::SwitchEnd, region);
    }

    fn visit_try_stmt(&mut self, stmt: &TryStmt) {
        let region = self.next_control_region();
        self.emit_control(stmt.span(), ControlKind::TryStart, region);
        stmt.block.visit_with(self);
        if let Some(handler) = &stmt.handler {
            self.emit_control(handler.span(), ControlKind::CatchStart, region);
            handler.visit_with(self);
        }
        if let Some(finalizer) = &stmt.finalizer {
            self.emit_control(finalizer.span(), ControlKind::FinallyStart, region);
            finalizer.visit_with(self);
        }
        self.emit_control(stmt.span(), ControlKind::TryEnd, region);
    }

    fn visit_cond_expr(&mut self, expr: &CondExpr) {
        let region = self.next_control_region();
        self.emit_control(expr.span(), ControlKind::BranchStart, region);
        expr.test.visit_with(self);
        self.emit_control(expr.cons.span(), ControlKind::BranchThen, region);
        expr.cons.visit_with(self);
        self.emit_control(expr.alt.span(), ControlKind::BranchElse, region);
        expr.alt.visit_with(self);
        self.emit_control(expr.span(), ControlKind::BranchEnd, region);
    }

    fn visit_break_stmt(&mut self, stmt: &swc_ecma_ast::BreakStmt) {
        self.emit_control(stmt.span(), ControlKind::Break, 0);
    }

    fn visit_continue_stmt(&mut self, stmt: &swc_ecma_ast::ContinueStmt) {
        self.emit_control(stmt.span(), ControlKind::Continue, 0);
    }

    fn visit_return_stmt(&mut self, stmt: &swc_ecma_ast::ReturnStmt) {
        stmt.arg.visit_with(self);
        self.emit_control(stmt.span(), ControlKind::Return, 0);
    }

    fn visit_export_decl(&mut self, export: &ExportDecl) {
        export.decl.visit_with(self);
    }
}

#[cfg(test)]
pub(super) fn build_test_stream(
    program: &swc_ecma_ast::Program,
    resolver: &Resolver,
) -> FactStream {
    let mut builder = FactBuilder::new(resolver);
    program.visit_with(&mut builder);
    builder.into_stream()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_builder_emits_facts_for_diverse_program() {
        let src = r#"
            const x = 1;
            function foo(a) {
                const y = a + x;
                return y;
            }
            foo(2);
            const obj = { prop: 3 };
            obj.prop = 4;
            new Error("fail");
        "#;
        let parsed = crate::parse(src, "fact-builder.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let facts = stream.facts();

        assert!(!facts.is_empty(), "fact builder should emit facts");

        let kinds: Vec<_> = facts.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&FactKind::Declaration));
        assert!(kinds.contains(&FactKind::Call));
        assert!(kinds.contains(&FactKind::PropertyWrite));
        assert!(kinds.contains(&FactKind::MemberRead));
    }

    #[test]
    fn facts_record_the_lexical_function_owner() {
        let parsed = crate::parse("fetch(); function helper() { fetch(); }", "owners.js")
            .expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let calls = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::Call)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].function, calls[1].function);
    }

    #[test]
    fn fact_ids_are_sequential_and_deterministic() {
        let src = "const a = 1; const b = 2; foo();";
        let parsed = crate::parse(src, "ids.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder1 = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder1);
        let stream1 = builder1.into_stream();

        let mut builder2 = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder2);
        let stream2 = builder2.into_stream();

        let ids1: Vec<_> = stream1.facts().iter().map(|f| f.id.0).collect();
        let ids2: Vec<_> = stream2.facts().iter().map(|f| f.id.0).collect();
        assert_eq!(
            ids1, ids2,
            "identical programs must produce identical fact IDs"
        );
        assert_eq!(
            ids1,
            (0..ids1.len() as u32).collect::<Vec<_>>(),
            "IDs must be sequential from 0"
        );
    }

    #[test]
    fn fact_count_is_independent_of_enabled_rules() {
        let src = "fetch('/api'); document.createElement('div');";
        let parsed = crate::parse(src, "invariant.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let count = stream.len();

        let mut builder2 = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder2);
        let stream2 = builder2.into_stream();
        assert_eq!(
            count,
            stream2.len(),
            "fact count must be invariant across runs"
        );
        assert_eq!(
            stream.fingerprint(),
            stream2.fingerprint(),
            "fact payloads and IDs must be invariant across runs"
        );
    }

    #[test]
    fn optional_chain_does_not_double_record_roles() {
        let src = "foo?.bar?.baz();";
        let parsed = crate::parse(src, "opt.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let facts = stream.facts();

        let call_facts: Vec<_> = facts.iter().filter(|f| f.kind == FactKind::Call).collect();
        assert_eq!(
            call_facts.len(),
            1,
            "optional call must emit exactly one Call fact"
        );

        let member_facts: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == FactKind::MemberRead)
            .collect();
        assert!(
            member_facts.len() <= 3,
            "optional member chain should not over-produce MemberRead facts, got {}",
            member_facts.len()
        );
    }

    #[test]
    fn nested_call_and_member_roles_have_distinct_facts() {
        let parsed =
            crate::parse("outer(inner(value.prop));", "nested.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let calls = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::Call)
            .collect::<Vec<_>>();
        let members = stream
            .facts()
            .iter()
            .filter(|fact| fact.kind == FactKind::MemberRead)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_eq!(members.len(), 1);
        assert!(calls[0].id != calls[1].id);
        assert!(members[0].span.lo >= calls[0].span.lo);
        assert!(members[0].span.hi <= calls[0].span.hi);
    }

    #[test]
    fn repeated_builds_yield_identical_fact_fingerprints() {
        let src = r#"
            const a = fetch('https://example.com');
            a.then(x => x.json());
            document.getElementById('root');
        "#;
        let parsed = crate::parse(src, "fp.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);

        let build_facts = || {
            let mut builder = FactBuilder::new(&resolver);
            parsed.program.visit_with(&mut builder);
            let stream = builder.into_stream();
            stream
                .facts()
                .iter()
                .map(|f| (f.kind, f.span.lo.0, f.span.hi.0, f.function, f.scope))
                .collect::<Vec<_>>()
        };

        let fp1 = build_facts();
        let fp2 = build_facts();
        let fp3 = build_facts();
        assert_eq!(
            fp1, fp2,
            "repeated builds must produce identical fingerprints"
        );
        assert_eq!(
            fp2, fp3,
            "repeated builds must produce identical fingerprints"
        );
    }

    #[test]
    fn call_fact_captures_callee_provenance() {
        let src = "fetch('/api');";
        let parsed = crate::parse(src, "call-prov.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| f.kind == FactKind::Call)
            .collect();
        assert_eq!(call_facts.len(), 1);
        if let FactPayload::Call {
            call_provenance,
            callee_name,
            ..
        } = &call_facts[0].payload
        {
            assert!(
                matches!(call_provenance, SymbolCallProvenance::Global { name } if name == "fetch"),
                "fetch should resolve to global provenance"
            );
            assert_eq!(callee_name.as_deref(), Some("fetch"));
        } else {
            panic!("expected Call payload");
        }
    }

    #[test]
    fn member_read_fact_captures_chain_info() {
        let src = "const x = document.body;";
        let parsed = crate::parse(src, "member-prov.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let member_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::MemberRead { .. }))
            .collect();
        assert!(!member_facts.is_empty(), "should have member read facts");
        if let FactPayload::MemberRead { rooted_chain, .. } = &member_facts[0].payload {
            assert!(
                rooted_chain.is_some(),
                "document.body should have a rooted chain"
            );
        }
    }

    #[test]
    fn import_fact_is_emitted() {
        let src = r#"import { x } from 'module';"#;
        let parsed = crate::parse(src, "import.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let import_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Import { .. }))
            .collect();
        assert_eq!(import_facts.len(), 1);
        if let FactPayload::Import { module } = &import_facts[0].payload {
            assert_eq!(module, "module");
        }
    }

    #[test]
    fn string_literal_fact_is_emitted() {
        let src = r#"const x = "hello";"#;
        let parsed = crate::parse(src, "str.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let str_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| {
                matches!(
                    &f.payload,
                    FactPayload::Reference {
                        static_string: Some(_),
                        ..
                    }
                )
            })
            .collect();
        assert!(!str_facts.is_empty(), "should have string literal facts");
        let values: Vec<_> = str_facts
            .iter()
            .filter_map(|f| {
                if let FactPayload::Reference {
                    static_string: Some(value),
                    ..
                } = &f.payload
                {
                    Some(value.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            values.contains(&"hello"),
            "should find 'hello' string literal"
        );
    }

    #[test]
    fn class_fact_is_emitted_for_class_declaration() {
        let src = r#"class Foo extends Bar {}"#;
        let parsed = crate::parse(src, "class.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let class_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Class { .. }))
            .collect();
        assert!(!class_facts.is_empty(), "should have class facts");
        if let FactPayload::Class { name, .. } = &class_facts[0].payload {
            assert_eq!(name, "Foo");
        }
    }

    #[test]
    fn instance_class_is_captured_for_this_calls() {
        let src = r#"
            import { Base } from 'lib';
            class Foo extends Base {
                bar() { this.baz(); }
            }
        "#;
        let parsed = crate::parse(src, "instance.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let mut builder = FactBuilder::new(&resolver);
        parsed.program.visit_with(&mut builder);
        let stream = builder.into_stream();
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| f.kind == FactKind::Call)
            .collect();
        let this_call = call_facts
            .iter()
            .find(|f| {
                if let FactPayload::Call { instance_class, .. } = &f.payload {
                    instance_class.is_some()
                } else {
                    false
                }
            })
            .expect("should find this.baz() call with instance_class");
        if let FactPayload::Call {
            instance_class,
            syntactic_chain,
            ..
        } = &this_call.payload
        {
            assert!(
                instance_class.is_some(),
                "this.baz() inside a class with module superclass should capture instance_class"
            );
            assert!(
                syntactic_chain.is_some(),
                "should have syntactic chain for member call"
            );
        }
    }
}
