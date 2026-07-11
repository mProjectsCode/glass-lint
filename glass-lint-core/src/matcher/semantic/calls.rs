//! AST collection for call, member, and constructor facts.
//!
//! The collector records three views of a call: its source spelling, any
//! canonical rooted chain, and module provenance. Keeping all three is what
//! lets `Any`, rooted, and module-qualified matchers remain precise without
//! each matcher revisiting the AST.

use std::borrow::Cow;

use swc_common::{DUMMY_SP, Spanned};
use swc_ecma_ast::{
    BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, ClassMethod, Expr, ExprOrSpread, Ident,
    ImportDecl, MemberExpr, NewExpr, ObjectLit, OptCall, OptChainBase, OptChainExpr, Program, Str,
    Tpl,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{CallMatcher, CallProvenance, MemberCallMatcher, MemberCallProvenance};
use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, expr_member, expr_name,
    member_prop_name, object_keys, prop_name,
};
use super::{index::MatcherFacts, resolver::Resolver};
use crate::matcher::rule::canonical_rooted_chain;

pub fn collect(
    program: &Program,
    resolver: &Resolver,
    member_argument_matchers: &[(usize, MemberCallMatcher)],
    call_argument_matchers: &[(usize, CallMatcher)],
    index: &mut MatcherFacts,
    argument_evidence: &mut [Vec<ApiEvidence>],
) {
    let mut visitor = ResolvedCallCollector {
        index,
        resolver,
        member_argument_matchers,
        call_argument_matchers,
        argument_evidence,
    };
    program.visit_with(&mut visitor);
}

struct ResolvedCallCollector<'a, 'rules> {
    index: &'a mut MatcherFacts,
    resolver: &'a Resolver,
    member_argument_matchers: &'rules [(usize, MemberCallMatcher)],
    call_argument_matchers: &'rules [(usize, CallMatcher)],
    argument_evidence: &'a mut [Vec<ApiEvidence>],
}

#[derive(Clone)]
struct CallArgumentSource<'a> {
    args: Cow<'a, [ExprOrSpread]>,
    span: swc_common::Span,
}

impl<'a> From<&'a CallExpr> for CallArgumentSource<'a> {
    fn from(call: &'a CallExpr) -> Self {
        Self {
            args: Cow::Borrowed(&call.args),
            span: call.span,
        }
    }
}

impl<'a> From<&'a OptCall> for CallArgumentSource<'a> {
    fn from(call: &'a OptCall) -> Self {
        Self {
            args: Cow::Borrowed(&call.args),
            span: call.span,
        }
    }
}

impl<'a> CallArgumentSource<'a> {
    fn with_args(args: Cow<'a, [ExprOrSpread]>, span: swc_common::Span) -> Self {
        Self { args, span }
    }

    fn prepend_bound_strings(mut self, bound: &[Option<String>]) -> Self {
        if bound.is_empty() {
            return self;
        }
        let mut args = bound
            .iter()
            .map(|value| ExprOrSpread {
                spread: None,
                expr: Box::new(match value {
                    Some(value) => Expr::Lit(swc_ecma_ast::Lit::Str(swc_ecma_ast::Str {
                        span: DUMMY_SP,
                        value: value.clone().into(),
                        raw: None,
                    })),
                    None => Expr::Invalid(Default::default()),
                }),
            })
            .collect::<Vec<_>>();
        args.extend(self.args.into_owned());
        self.args = Cow::Owned(args);
        self
    }
}

impl ResolvedCallCollector<'_, '_> {
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
                .bound_string_arguments(ident)
                .map_or(call.clone(), |bound| call.prepend_bound_strings(&bound))
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
        for (rule_index, matcher) in self.call_argument_matchers {
            let call_matches = match &matcher.provenance {
                CallProvenance::Any => ident.sym == *matcher.name,
                CallProvenance::Global => matches!(
                    found_provenance,
                    SymbolCallProvenance::Global { name } if name == &matcher.name
                ),
                CallProvenance::ModuleExport { module } => matches!(
                    found_provenance,
                    SymbolCallProvenance::ModuleExport {
                        module: found_module,
                        export
                    } if found_module == module && export == &matcher.name
                ),
            };

            if call_matches
                && matcher.arg_strings.iter().all(|arg_matcher| {
                    call.args.get(arg_matcher.index).is_some_and(|argument| {
                        let static_value = self.resolver.static_string_expr(&argument.expr);
                        static_value.is_some_and(|value| {
                            arg_matcher.predicate.as_ref().map_or_else(
                                || {
                                    arg_matcher.values.is_empty()
                                        || arg_matcher
                                            .values
                                            .iter()
                                            .any(|expected| expected == &value)
                                },
                                |predicate| {
                                    super::object_flow::matches_static_value(predicate, &value)
                                },
                            )
                        })
                    })
                })
            {
                self.argument_evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol: matcher.evidence_symbol(),
                    count: 1,
                    spans: vec![call.span],
                });
            }
        }
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
            "call" if !call_args.is_empty() => Some(CallArgumentSource::with_args(
                Cow::Borrowed(&call_args[1..]),
                span,
            )),
            "apply" if call_args.len() >= 2 => {
                if let Expr::Array(array) = &*call_args[1].expr {
                    if array.elems.iter().any(|element| {
                        element
                            .as_ref()
                            .is_none_or(|element| element.spread.is_some())
                    }) {
                        return;
                    }
                    Some(CallArgumentSource::with_args(
                        Cow::Owned(array.elems.iter().flatten().cloned().collect()),
                        span,
                    ))
                } else {
                    let Some(values) = self.resolver.static_string_array_expr(&call_args[1].expr)
                    else {
                        return;
                    };
                    Some(CallArgumentSource::with_args(
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
                    ))
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
        self.collect_argument_evidence(
            CallArgumentSource::from(call),
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
        for (rule_index, matcher) in self.member_argument_matchers {
            let member_matches = match &matcher.provenance {
                MemberCallProvenance::Any => syntactic_chain == Some(&matcher.chain),
                MemberCallProvenance::Rooted => resolved_chain
                    .map(canonical_rooted_chain)
                    .is_some_and(|chain| chain == matcher.chain),
                MemberCallProvenance::ModuleNamespace { module } => matches!(
                    module_member,
                    Some(SymbolMemberProvenance::ModuleNamespace {
                        module: found_module,
                        member
                    }) if found_module == module && member == &matcher.chain
                ),
            };
            if member_matches
                && matcher.arg_strings.iter().all(|arg_matcher| {
                    call.args.get(arg_matcher.index).is_some_and(|argument| {
                        self.resolver
                            .static_string_expr(&argument.expr)
                            .is_some_and(|value| {
                                arg_matcher.predicate.as_ref().map_or_else(
                                    || {
                                        arg_matcher.values.is_empty()
                                            || arg_matcher
                                                .values
                                                .iter()
                                                .any(|expected| expected == &value)
                                    },
                                    |predicate| {
                                        super::object_flow::matches_static_value(predicate, &value)
                                    },
                                )
                            })
                    })
                })
                && matcher.arg_object_keys.iter().all(|key_matcher| {
                    call.args
                        .get(key_matcher.index)
                        .and_then(|argument| {
                            self.resolver
                                .object_keys_expr(&argument.expr)
                                .or_else(|| object_literal(&argument.expr).and_then(object_keys))
                        })
                        .is_some_and(|keys| {
                            key_matcher
                                .keys
                                .iter()
                                .all(|expected| keys.iter().any(|key| key == expected))
                        })
                })
                && matcher.arg_rooted_exprs.iter().all(|root_matcher| {
                    call.args
                        .get(root_matcher.index)
                        .and_then(|argument| self.resolver.rooted_expr_chain(&argument.expr))
                        .map(|chain| canonical_rooted_chain(&chain).to_string())
                        .is_some_and(|chain| {
                            root_matcher
                                .chains
                                .iter()
                                .any(|expected| expected == &chain)
                        })
                })
            {
                let symbol = match matcher.provenance {
                    MemberCallProvenance::Any => matcher.evidence_symbol(),
                    MemberCallProvenance::Rooted | MemberCallProvenance::ModuleNamespace { .. } => {
                        syntactic_chain.unwrap_or(&matcher.chain).to_string()
                    }
                };
                self.argument_evidence[*rule_index].push(ApiEvidence {
                    kind: ApiMatchKind::CallArgument,
                    symbol,
                    count: 1,
                    spans: vec![call.span],
                });
            }
        }
    }

    fn record_module_class_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(ident) => {
                if let SymbolCallProvenance::ModuleExport { module, export } =
                    self.resolver.resolve_ident(ident).call
                {
                    self.index
                        .record(ApiMatchKind::Class, export.clone(), ident.span);
                    self.index
                        .module_classes
                        .entry((module.clone(), export.clone()))
                        .or_default()
                        .push(ident.span);
                }
            }
            Expr::Member(member) => {
                if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                    self.resolver.resolve_member(member).module_member
                {
                    self.index
                        .record(ApiMatchKind::Class, member.clone(), expr.span());
                    self.index
                        .module_classes
                        .entry((module.clone(), member.clone()))
                        .or_default()
                        .push(expr.span());
                }
            }
            Expr::Paren(paren) => self.record_module_class_expr(&paren.expr),
            _ => {}
        }
    }
}

impl Visit for ResolvedCallCollector<'_, '_> {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        self.index
            .record(ApiMatchKind::Import, module, import.src.span);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
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
        self.record_member_read(member);

        member.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        let callee = super::ast::effective_callee_expr(&new_expr.callee);
        match callee {
            Expr::Ident(ident) => match self.resolver.resolve_ident(ident) {
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
                    self.index
                        .record(ApiMatchKind::Constructor, name.clone(), ident.span);
                    self.index
                        .global_constructors
                        .entry(name)
                        .or_default()
                        .push(ident.span);
                }
                resolved => match resolved.call {
                    SymbolCallProvenance::Global { name } => {
                        self.index
                            .record(ApiMatchKind::Constructor, name.clone(), ident.span);
                        self.index
                            .global_constructors
                            .entry(name)
                            .or_default()
                            .push(ident.span);
                    }
                    SymbolCallProvenance::ModuleExport { module, export } => {
                        self.index
                            .record(ApiMatchKind::Constructor, export.clone(), ident.span);
                        self.index
                            .module_constructors
                            .entry((module.clone(), export.clone()))
                            .or_default()
                            .push(ident.span);
                    }
                    SymbolCallProvenance::Local => {}
                },
            },
            Expr::Member(member) => {
                let resolved = self.resolver.resolve_member(member);
                let global_name = resolved.rooted_chain.as_deref().and_then(|chain| {
                    chain
                        .strip_prefix("globalThis.")
                        .filter(|_| matches!(self.resolver.resolve_expr(&member.obj).call, SymbolCallProvenance::Global { ref name } if name == "globalThis"))
                        .or((chain == "Function").then_some(chain))
                });
                if let Some(name) = global_name {
                    self.index
                        .record(ApiMatchKind::Constructor, name, new_expr.callee.span());
                    self.index
                        .global_constructors
                        .entry(name.to_string())
                        .or_default()
                        .push(new_expr.callee.span());
                } else if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                    resolved.module_member
                {
                    self.index.record(
                        ApiMatchKind::Constructor,
                        member.clone(),
                        new_expr.callee.span(),
                    );
                    self.index
                        .module_constructors
                        .entry((module.clone(), member.clone()))
                        .or_default()
                        .push(new_expr.callee.span());
                }
            }
            _ => {}
        }

        new_expr.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        if let Some(super_class) = class_decl.class.super_class.as_deref() {
            self.record_module_class_expr(super_class);
        }

        class_decl.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr) {
        if let Some(super_class) = class_expr.class.super_class.as_deref() {
            self.record_module_class_expr(super_class);
        }

        class_expr.visit_children_with(self);
    }

    fn visit_class_method(&mut self, method: &ClassMethod) {
        if let Some(name) = prop_name(&method.key) {
            self.index
                .record(ApiMatchKind::MemberRead, name, method.key.span());
        }

        method.visit_children_with(self);
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

fn object_literal(expr: &Expr) -> Option<&ObjectLit> {
    match expr {
        Expr::Object(object) => Some(object),
        Expr::Paren(paren) => object_literal(&paren.expr),
        _ => None,
    }
}
