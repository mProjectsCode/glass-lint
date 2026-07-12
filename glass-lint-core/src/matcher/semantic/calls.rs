//! AST collection for call, member, and constructor facts.
//!
//! The collector records three views of a call: its source spelling, any
//! canonical rooted chain, and module provenance. Keeping all three is what
//! lets `Any`, rooted, and module-qualified matchers remain precise without
//! each matcher revisiting the AST.

use std::borrow::Cow;

use swc_common::{DUMMY_SP, Spanned};
use swc_ecma_ast::{
    BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, ClassMethod, Expr, ExprOrSpread, FnDecl,
    Function, Ident, ImportDecl, MemberExpr, NewExpr, OptCall, OptChainBase, OptChainExpr, Program,
    Str, Tpl,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{CallMatcher, InstanceMemberCallMatcher, MemberCallMatcher};
use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, expr_member, expr_name,
    member_prop_name, prop_name,
};
use super::call::ResolvedCall;
use super::events::EventLog;
use super::{index::MatcherFacts, resolver::Resolver};
use crate::matcher::rule::canonical_rooted_chain;

pub fn collect(
    program: &Program,
    context: CallContext<'_>,
    member_argument_matchers: &[(usize, &MemberCallMatcher)],
    call_argument_matchers: &[(usize, &CallMatcher)],
    instance_matchers: &[&InstanceMemberCallMatcher],
    index: &mut MatcherFacts,
    argument_evidence: &mut [Vec<ApiEvidence>],
) {
    let mut visitor = ResolvedCallCollector {
        index,
        events: context.events,
        resolver: context.resolver,
        member_argument_matchers,
        call_argument_matchers,
        instance_matchers,
        argument_evidence,
        classes: Vec::new(),
        ordinary_functions: 0,
        static_methods: 0,
    };
    program.visit_with(&mut visitor);
}

struct ResolvedCallCollector<'a, 'rules> {
    index: &'a mut MatcherFacts,
    events: &'a EventLog,
    resolver: &'a Resolver,
    member_argument_matchers: &'rules [(usize, &'rules MemberCallMatcher)],
    call_argument_matchers: &'rules [(usize, &'rules CallMatcher)],
    instance_matchers: &'rules [&'rules InstanceMemberCallMatcher],
    argument_evidence: &'a mut [Vec<ApiEvidence>],
    classes: Vec<Option<(String, String)>>,
    ordinary_functions: usize,
    static_methods: usize,
}

type CallArgumentSource<'a> = ResolvedCall<'a>;

pub(super) struct CallContext<'a> {
    pub(super) events: &'a EventLog,
    pub(super) resolver: &'a Resolver,
}

impl ResolvedCallCollector<'_, '_> {
    fn record_instance_call(&mut self, member: &MemberExpr, span: swc_common::Span) {
        let mut context = super::constructors::InstanceContext {
            index: self.index,
            resolver: self.resolver,
            instance_matchers: self.instance_matchers,
            classes: &self.classes,
            ordinary_functions: self.ordinary_functions,
            static_methods: self.static_methods,
        };
        context.record_instance_call(member, span);
    }

    /// Visit the children of a callee without visiting the callee expression
    /// itself.  The latter is already represented by the resolved call fact;
    /// visiting it again would classify a call target as an ordinary member
    /// read.
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

    fn record_identifier_call(&mut self, ident: &Ident, call: Option<CallArgumentSource<'_>>) {
        let name = ident.sym.to_string();
        self.index
            .record(ApiMatchKind::Call, name.clone(), ident.span);

        let resolved = self.resolver.resolve_ident(ident);
        if !self.resolver.value_is_known(resolved.id) {
            return;
        }
        let provenance = resolved.call;
        let aliased_member = resolved.rooted_chain;
        let call = call.map(|call| {
            self.resolver
                .bound_arguments(ident)
                .map_or(call.clone(), |bound| call.prepend_bound_arguments(&bound))
        });
        if let Some(call) = call {
            self.collect_call_argument_evidence(call.clone(), ident, &provenance);
            if let Some(chain) = aliased_member.as_deref() {
                let module_member = resolved.module_member;
                self.collect_argument_evidence(
                    call,
                    Some(chain),
                    Some(chain),
                    module_member.as_ref(),
                );
            }
        }

        if let Some(chain) = aliased_member {
            self.index
                .rooted_member_calls
                .entry(canonical_rooted_chain(&chain).to_string())
                .or_default()
                .push(ident.span);
        }

        match provenance {
            SymbolCallProvenance::Global { name } => {
                self.index
                    .global_calls
                    .entry(name)
                    .or_default()
                    .push(ident.span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.index
                    .module_calls
                    .entry((module.clone(), export.clone()))
                    .or_default()
                    .push(ident.span);
                self.index
                    .module_member_calls
                    .entry((module, export))
                    .or_default()
                    .push(ident.span);
            }
            SymbolCallProvenance::Local => {}
        }
    }

    fn collect_call_argument_evidence(
        &mut self,
        call: CallArgumentSource<'_>,
        ident: &Ident,
        found_provenance: &SymbolCallProvenance,
    ) {
        super::call_arguments::collect_call_argument_evidence(
            self.resolver,
            self.call_argument_matchers,
            self.argument_evidence,
            call,
            ident,
            found_provenance,
        );
    }

    fn record_member_call(&mut self, member: &MemberExpr, call: Option<CallArgumentSource<'_>>) {
        let resolved = self.resolver.resolve_member(member);
        if resolved.rooted_chain.as_deref() == Some("Function") {
            self.index
                .record(ApiMatchKind::Call, "Function", member.span);
            self.index
                .global_calls
                .entry("Function".to_string())
                .or_default()
                .push(member.span);
        }
        let syntactic_chain = self.resolver.member_chain(member);
        if !self.resolver.value_is_known(resolved.id) {
            return;
        }
        let resolved_chain = resolved.rooted_chain;
        let module_member = resolved.module_member;
        if let Some((source, member_name)) = resolved.returned_member.clone() {
            self.index
                .returned_member_calls
                .entry((source, member_name))
                .or_default()
                .push(member.span);
        }

        if let Some(call) = call {
            self.collect_argument_evidence(
                call,
                syntactic_chain.as_deref(),
                resolved_chain.as_deref(),
                module_member.as_ref(),
            );
        }
        if let Some(chain) = syntactic_chain {
            self.index
                .record(ApiMatchKind::MemberCall, chain.clone(), member.span);
        }
        if let Some(chain) = resolved_chain {
            let chain = canonical_rooted_chain(&chain).to_string();
            self.index
                .rooted_member_calls
                .entry(chain.clone())
                .or_default()
                .push(member.span);
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace {
            module,
            member: member_name,
        }) = module_member
        {
            self.index
                .module_calls
                .entry((module.clone(), member_name.clone()))
                .or_default()
                .push(member.span);
            self.index
                .module_member_calls
                .entry((module.clone(), member_name.clone()))
                .or_default()
                .push(member.span);
        }
    }

    /// Resolve the target of `target.call(receiver, ...args)` and
    /// `target.apply(receiver, args)`.  The wrapper receiver is not an
    /// argument to the target, and `.apply` exposes arguments only when the
    /// array is statically bounded.
    fn record_callable_wrapper(&mut self, member: &MemberExpr, call: &CallExpr) {
        self.record_callable_wrapper_args(member, &call.args, call.span);
    }

    fn record_callable_wrapper_args(
        &mut self,
        member: &MemberExpr,
        call_args: &[ExprOrSpread],
        span: swc_common::Span,
    ) {
        let Some(property) = member_prop_name(&member.prop) else {
            return;
        };
        let args = match property.as_str() {
            "call" if !call_args.is_empty() => Some(
                CallArgumentSource::with_target(&member.obj, Cow::Borrowed(&call_args[1..]), span)
                    .with_receiver(&call_args[0].expr),
            ),
            "apply" if call_args.len() >= 2 => {
                if let Expr::Array(array) = &*call_args[1].expr {
                    if array.elems.iter().any(|element| {
                        element
                            .as_ref()
                            .is_none_or(|element| element.spread.is_some())
                    }) {
                        return;
                    }
                    Some(
                        CallArgumentSource::with_target(
                            &member.obj,
                            Cow::Owned(array.elems.iter().flatten().cloned().collect()),
                            span,
                        )
                        .with_receiver(&call_args[0].expr),
                    )
                } else {
                    let Some(values) = self.resolver.static_string_array_expr(&call_args[1].expr)
                    else {
                        return;
                    };
                    Some(
                        CallArgumentSource::with_target(
                            &member.obj,
                            Cow::Owned(
                                values
                                    .into_iter()
                                    .map(|value| ExprOrSpread {
                                        spread: None,
                                        expr: Box::new(Expr::Lit(swc_ecma_ast::Lit::Str(
                                            swc_ecma_ast::Str {
                                                span: DUMMY_SP,
                                                value: value.into(),
                                                raw: None,
                                            },
                                        ))),
                                    })
                                    .collect(),
                            ),
                            span,
                        )
                        .with_receiver(&call_args[0].expr),
                    )
                }
            }
            _ => None,
        };
        let Some(args) = args else {
            return;
        };
        self.record_resolved_target(&member.obj, args);
    }

    fn record_resolved_target(&mut self, target: &Expr, args: CallArgumentSource<'_>) {
        match effective_callee_expr(target) {
            Expr::Ident(ident) => self.record_identifier_call(ident, Some(args)),
            Expr::Member(member) => self.record_member_call(member, Some(args)),
            target => {
                let raw = expr_name(target);
                let rooted = self
                    .resolver
                    .rooted_expr_chain(target)
                    .map(|chain| canonical_rooted_chain(&chain).to_string());
                let module_member = expr_member(target)
                    .and_then(|member| self.resolver.resolve_member(member).module_member);
                self.collect_argument_evidence(
                    args,
                    raw.as_deref(),
                    rooted.as_deref(),
                    module_member.as_ref(),
                );
            }
        }
    }

    fn record_optional_target(&mut self, call: &OptCall) {
        let raw = expr_name(&call.callee);
        let rooted = self
            .resolver
            .rooted_expr_chain(&call.callee)
            .map(|chain| canonical_rooted_chain(&chain).to_string());
        let module_member = expr_member(&call.callee)
            .and_then(|member| self.resolver.resolve_member(member).module_member);
        let resolved_call = CallArgumentSource::from(call);
        debug_assert!(resolved_call.optional);
        self.collect_argument_evidence(
            resolved_call,
            raw.as_deref(),
            rooted.as_deref(),
            module_member.as_ref(),
        );
        if let Some(raw) = raw {
            self.index
                .record(ApiMatchKind::MemberCall, raw, call.callee.span());
        }
        if let Some(rooted) = rooted {
            self.index
                .rooted_member_calls
                .entry(rooted)
                .or_default()
                .push(call.callee.span());
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.index
                .module_member_calls
                .entry((module, member))
                .or_default()
                .push(call.callee.span());
        }
    }

    fn record_member_read(&mut self, member: &MemberExpr) {
        let syntactic_chain = self.resolver.member_chain(member);
        if let Some(chain) = syntactic_chain.as_ref() {
            let chain = canonical_rooted_chain(chain).to_string();
            self.index
                .record(ApiMatchKind::MemberRead, chain, member.span);
        }
        let resolved = self.resolver.resolve_member(member);
        if let Some((source, member_name)) = resolved.returned_member.clone() {
            self.index
                .returned_member_reads
                .entry((source, member_name))
                .or_default()
                .push(member.span);
        }
        if let Some(resolved_chain) = resolved.rooted_chain {
            self.index
                .rooted_member_reads
                .entry(canonical_rooted_chain(&resolved_chain).to_string())
                .or_default()
                .push(member.span);
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace {
            module,
            member: member_name,
        }) = resolved.module_member
        {
            self.index
                .module_member_reads
                .entry((module.clone(), member_name.clone()))
                .or_default()
                .push(member.span);
            self.index
                .record(ApiMatchKind::Class, member_name, member.span);
        }
    }

    fn collect_argument_evidence(
        &mut self,
        call: CallArgumentSource<'_>,
        syntactic_chain: Option<&str>,
        resolved_chain: Option<&str>,
        module_member: Option<&SymbolMemberProvenance>,
    ) {
        super::call_arguments::collect_member_argument_evidence(
            self.resolver,
            self.member_argument_matchers,
            self.argument_evidence,
            call,
            syntactic_chain,
            resolved_chain,
            module_member,
        );
    }

    fn record_module_class_expr(&mut self, expr: &Expr) {
        super::constructors::record_module_class_expr(self.index, self.resolver, expr);
    }
}

impl Visit for ResolvedCallCollector<'_, '_> {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        self.index
            .record(ApiMatchKind::Import, module, import.src.span);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if self.events.order_for(call.span).is_none() {
            return;
        }
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Member(member) = effective_callee_expr(callee)
        {
            self.record_instance_call(member, call.span);
        }
        if let Some(module) = self.resolver.require_module_name(call) {
            self.index.record(ApiMatchKind::Import, module, call.span);
        }

        match &call.callee {
            Callee::Expr(callee) => match effective_callee_expr(callee) {
                Expr::Ident(ident) => {
                    self.record_identifier_call(ident, Some(CallArgumentSource::from(call)));
                }
                Expr::Member(member) => {
                    if matches!(
                        member_prop_name(&member.prop).as_deref(),
                        Some("call" | "apply")
                    ) {
                        self.record_member_call(member, None);
                        self.record_callable_wrapper(member, call);
                    } else {
                        self.record_member_call(member, Some(CallArgumentSource::from(call)));
                    }
                }
                Expr::OptChain(chain) => {
                    if let OptChainBase::Member(member) = &*chain.base {
                        if matches!(
                            member_prop_name(&member.prop).as_deref(),
                            Some("call" | "apply")
                        ) {
                            self.record_member_call(member, None);
                            self.record_callable_wrapper(member, call);
                        } else {
                            self.record_member_call(member, Some(CallArgumentSource::from(call)));
                        }
                    }
                }
                other => {
                    if let Some(provenance) = self.resolver.expr_call_provenance(other) {
                        match provenance {
                            SymbolCallProvenance::Global { name } => {
                                self.index
                                    .global_calls
                                    .entry(name.clone())
                                    .or_default()
                                    .push(call.span);
                                self.index.record(ApiMatchKind::Call, name, call.span);
                            }
                            SymbolCallProvenance::ModuleExport { module, export } => {
                                self.index
                                    .module_calls
                                    .entry((module.clone(), export.clone()))
                                    .or_default()
                                    .push(call.span);
                                self.index.record(ApiMatchKind::Call, export, call.span);
                            }
                            SymbolCallProvenance::Local => {}
                        }
                    }
                }
            },
            Callee::Super(_) => self.index.record(ApiMatchKind::Call, "super", call.span),
            Callee::Import(_) => self.index.record(ApiMatchKind::Call, "import", call.span),
        }

        if let Callee::Expr(callee) = &call.callee {
            self.visit_callee_children(callee);
        }
        call.args.visit_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        if self.events.order_for(chain.span()).is_none() {
            return;
        }
        match &*chain.base {
            OptChainBase::Call(call) => match &*call.callee {
                Expr::Ident(ident) => {
                    self.record_identifier_call(ident, Some(CallArgumentSource::from(call)))
                }
                Expr::Member(member) => {
                    if matches!(
                        member_prop_name(&member.prop).as_deref(),
                        Some("call" | "apply")
                    ) {
                        self.record_member_call(member, None);
                        self.record_callable_wrapper_args(member, &call.args, call.span);
                    } else {
                        self.record_member_call(member, Some(CallArgumentSource::from(call)))
                    }
                }
                Expr::OptChain(chain) => {
                    if let OptChainBase::Member(member) = &*chain.base {
                        if matches!(
                            member_prop_name(&member.prop).as_deref(),
                            Some("call" | "apply")
                        ) {
                            self.record_member_call(member, None);
                            self.record_callable_wrapper_args(member, &call.args, call.span);
                        } else {
                            self.record_optional_target(call);
                        }
                    }
                }
                _ => self.record_optional_target(call),
            },
            OptChainBase::Member(member) => self.record_member_read(member),
        }
        match &*chain.base {
            OptChainBase::Call(call) => {
                self.visit_callee_children(&call.callee);
                call.args.visit_with(self);
            }
            OptChainBase::Member(member) => {
                member.obj.visit_with(self);
                member.prop.visit_with(self);
            }
        }
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if self.events.order_for(member.span).is_none() {
            return;
        }
        self.record_member_read(member);

        member.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        super::constructors::record_new_expr(self.index, self.resolver, new_expr);
        new_expr.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        if let Some(super_class) = class_decl.class.super_class.as_deref() {
            self.record_module_class_expr(super_class);
        }
        let origin = class_decl
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.classes.push(origin);
        class_decl.visit_children_with(self);
        self.classes.pop();
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr) {
        if let Some(super_class) = class_expr.class.super_class.as_deref() {
            self.record_module_class_expr(super_class);
        }
        let origin = class_expr
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.classes.push(origin);
        class_expr.visit_children_with(self);
        self.classes.pop();
    }

    fn visit_class_method(&mut self, method: &ClassMethod) {
        if let Some(name) = prop_name(&method.key) {
            self.index
                .record(ApiMatchKind::MemberRead, name, method.key.span());
        }
        self.static_methods += usize::from(method.is_static);
        if let Some(body) = method.function.body.as_ref() {
            body.visit_with(self);
        }
        self.static_methods -= usize::from(method.is_static);
    }

    fn visit_fn_decl(&mut self, function: &FnDecl) {
        self.ordinary_functions += 1;
        function.visit_children_with(self);
        self.ordinary_functions -= 1;
    }

    fn visit_function(&mut self, function: &Function) {
        self.ordinary_functions += 1;
        function.visit_children_with(self);
        self.ordinary_functions -= 1;
    }

    fn visit_bin_expr(&mut self, binary: &swc_ecma_ast::BinExpr) {
        if binary.op == BinaryOp::InstanceOf {
            self.record_module_class_expr(&binary.right);
        }

        binary.visit_children_with(self);
    }

    fn visit_str(&mut self, value: &Str) {
        let literal = value.value.to_string_lossy().to_string();
        self.index
            .record(ApiMatchKind::StringLiteral, literal, value.span);

        value.visit_children_with(self);
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        for quasi in &template.quasis {
            let literal = quasi
                .cooked
                .as_ref()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| quasi.raw.to_string());
            self.index
                .record(ApiMatchKind::StringLiteral, literal, quasi.span);
        }

        template.visit_children_with(self);
    }
}
