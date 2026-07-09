use std::collections::BTreeMap;

use swc_common::Spanned;
use swc_ecma_ast::{
    BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, ClassMethod, Expr, ExprOrSpread, Ident,
    ImportDecl, MemberExpr, NewExpr, ObjectLit, OptCall, OptChainBase, OptChainExpr, Program, Str,
    Tpl,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::alias::AliasInfo;
use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, expr_member, expr_name,
    is_function_constructor_member, member_chain, object_keys, prop_name, require_call_module_name,
};
use super::{
    ApiEvidence, ApiMatchKind, CallMatcher, CallProvenance, MemberCallMatcher,
    MemberCallProvenance, SymbolIndex,
};
use crate::matcher::rule::canonical_rooted_chain;

pub fn collect(
    program: &Program,
    aliases: &AliasInfo,
    member_argument_matchers: &[(usize, MemberCallMatcher)],
    call_argument_matchers: &[(usize, CallMatcher)],
    index: &mut SymbolIndex,
    argument_evidence: &mut [Vec<ApiEvidence>],
) {
    let mut visitor = SymbolIndexVisitor {
        index,
        aliases,
        pending_callee_reads: BTreeMap::new(),
        member_argument_matchers,
        call_argument_matchers,
        argument_evidence,
    };
    program.visit_with(&mut visitor);
}

struct SymbolIndexVisitor<'a, 'rules> {
    index: &'a mut SymbolIndex,
    aliases: &'a AliasInfo,
    pending_callee_reads: BTreeMap<String, u32>,
    member_argument_matchers: &'rules [(usize, MemberCallMatcher)],
    call_argument_matchers: &'rules [(usize, CallMatcher)],
    argument_evidence: &'a mut [Vec<ApiEvidence>],
}

#[derive(Clone, Copy)]
struct CallArgumentSource<'a> {
    args: &'a [ExprOrSpread],
    span: swc_common::Span,
}

impl<'a> From<&'a CallExpr> for CallArgumentSource<'a> {
    fn from(call: &'a CallExpr) -> Self {
        Self {
            args: &call.args,
            span: call.span,
        }
    }
}

impl<'a> From<&'a OptCall> for CallArgumentSource<'a> {
    fn from(call: &'a OptCall) -> Self {
        Self {
            args: &call.args,
            span: call.span,
        }
    }
}

impl SymbolIndexVisitor<'_, '_> {
    fn record_identifier_call(&mut self, ident: &Ident, call: Option<CallArgumentSource<'_>>) {
        let name = ident.sym.to_string();
        self.index
            .record(ApiMatchKind::Call, name.clone(), ident.span);

        let provenance = self.aliases.call_provenance(&name, ident.span);
        if let Some(call) = call {
            self.collect_call_argument_evidence(call, ident, &provenance);
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
                        self.aliases
                            .static_string_expr(&argument.expr)
                            .is_some_and(|value| {
                                arg_matcher.values.is_empty()
                                    || arg_matcher.values.iter().any(|expected| expected == &value)
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
        if is_function_constructor_member(member) {
            self.index
                .record(ApiMatchKind::Call, "Function", member.span);
            self.index
                .global_calls
                .entry("Function".to_string())
                .or_default()
                .push(member.span);
        }
        let syntactic_chain = member_chain(member);
        let resolved_chain = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.resolve_member_chain(member, chain))
            .or_else(|| self.aliases.rooted_member_chain(member));
        let module_member = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.member_call_provenance_for_chain(member, chain));

        self.record_static_callable_wrapper(member);

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
            *self.pending_callee_reads.entry(chain.clone()).or_insert(0) += 1;
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
                .module_member_calls
                .entry((module.clone(), member_name.clone()))
                .or_default()
                .push(member.span);
        }
    }

    fn record_member_read(&mut self, member: &MemberExpr) {
        let syntactic_chain = member_chain(member);
        if let Some(chain) = syntactic_chain.as_ref() {
            let chain = canonical_rooted_chain(chain).to_string();
            self.index
                .record(ApiMatchKind::MemberRead, chain, member.span);
        }
        if let Some(resolved_chain) = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.resolve_member_chain(member, chain))
        {
            self.index
                .rooted_member_reads
                .entry(canonical_rooted_chain(&resolved_chain).to_string())
                .or_default()
                .push(member.span);
        }
        let module_member = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.member_call_provenance_for_chain(member, chain));
        if let Some(SymbolMemberProvenance::ModuleNamespace {
            module,
            member: member_name,
        }) = module_member
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
                        self.aliases
                            .static_string_expr(&argument.expr)
                            .is_some_and(|value| {
                                arg_matcher.values.is_empty()
                                    || arg_matcher.values.iter().any(|expected| expected == &value)
                            })
                    })
                })
                && matcher.arg_object_keys.iter().all(|key_matcher| {
                    call.args
                        .get(key_matcher.index)
                        .and_then(|argument| {
                            self.aliases
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
                        .and_then(|argument| self.aliases.rooted_expr_chain(&argument.expr))
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
                    self.aliases.call_provenance(ident.sym.as_ref(), ident.span)
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
                    self.aliases.member_call_provenance(member)
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

    fn record_static_callable_wrapper(&mut self, member: &MemberExpr) {
        let Some(property) = super::ast::member_prop_name(&member.prop) else {
            return;
        };
        if property != "call" && property != "apply" {
            return;
        }
        let Some(provenance) = self.aliases.expr_call_provenance(&member.obj) else {
            return;
        };
        match provenance {
            SymbolCallProvenance::Global { name } => {
                self.index
                    .global_calls
                    .entry(name.clone())
                    .or_default()
                    .push(member.span);
                self.index.record(ApiMatchKind::Call, name, member.span);
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                self.index
                    .module_calls
                    .entry((module.clone(), export.clone()))
                    .or_default()
                    .push(member.span);
                self.index.record(ApiMatchKind::Call, export, member.span);
            }
            SymbolCallProvenance::Local => {}
        }
    }
}

impl Visit for SymbolIndexVisitor<'_, '_> {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        self.index
            .record(ApiMatchKind::Import, module, import.src.span);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(module) = require_call_module_name(call) {
            self.index.record(ApiMatchKind::Import, module, call.span);
        }

        match &call.callee {
            Callee::Expr(callee) => match effective_callee_expr(callee) {
                Expr::Ident(ident) => {
                    self.record_identifier_call(ident, Some(CallArgumentSource::from(call)));
                }
                Expr::Member(member) => {
                    self.record_member_call(member, Some(CallArgumentSource::from(call)));
                }
                other => {
                    if let Some(provenance) = self.aliases.expr_call_provenance(other) {
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

        call.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        match &*chain.base {
            OptChainBase::Call(call) => match &*call.callee {
                Expr::Ident(ident) => {
                    self.record_identifier_call(ident, Some(CallArgumentSource::from(call)))
                }
                Expr::Member(member) => {
                    self.record_member_call(member, Some(CallArgumentSource::from(call)))
                }
                _ => {
                    let raw = expr_name(&call.callee);
                    let rooted = self
                        .aliases
                        .rooted_expr_chain(&call.callee)
                        .map(|chain| canonical_rooted_chain(&chain).to_string());
                    let module_member = expr_member(&call.callee)
                        .and_then(|member| self.aliases.member_call_provenance(member));
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
                            .entry(rooted.clone())
                            .or_default()
                            .push(call.callee.span());
                    }
                    if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                        module_member
                    {
                        self.index
                            .module_member_calls
                            .entry((module.clone(), member.clone()))
                            .or_default()
                            .push(call.callee.span());
                    }
                }
            },
            OptChainBase::Member(member) => self.record_member_read(member),
        }
        chain.visit_children_with(self);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        let syntactic_chain = member_chain(member);
        if let Some(chain) = syntactic_chain.as_ref()
            && let Some(skip_count) = self.pending_callee_reads.get_mut(chain.as_str())
        {
            *skip_count -= 1;
            if *skip_count == 0 {
                self.pending_callee_reads.remove(chain.as_str());
            }

            member.visit_children_with(self);
            return;
        }
        self.record_member_read(member);

        member.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        match &*new_expr.callee {
            Expr::Ident(ident) => {
                match self.aliases.call_provenance(ident.sym.as_ref(), ident.span) {
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
                }
            }
            Expr::Member(member) => {
                if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                    self.aliases.member_call_provenance(member)
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
