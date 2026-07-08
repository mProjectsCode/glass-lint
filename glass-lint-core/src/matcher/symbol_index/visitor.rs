use std::collections::BTreeMap;

use swc_ecma_ast::{
    BinaryOp, CallExpr, Callee, ClassDecl, ClassExpr, ClassMethod, Expr, Ident, ImportDecl,
    MemberExpr, NewExpr, ObjectLit, OptChainBase, OptChainExpr, Program, Prop, PropOrSpread, Str,
    Tpl,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::alias::AliasInfo;
use super::ast::{
    SymbolCallProvenance, SymbolMemberProvenance, effective_callee_expr, expr_member, expr_name,
    member_chain, prop_name, require_call_module_name, static_string,
};
use super::{ApiEvidence, ApiMatchKind, MemberCallMatcher, MemberCallProvenance, SymbolIndex};
use crate::matcher::rule::canonical_rooted_chain;

pub fn collect(
    program: &Program,
    aliases: &AliasInfo,
    argument_matchers: &[(usize, MemberCallMatcher)],
    index: &mut SymbolIndex,
    argument_evidence: &mut [Vec<ApiEvidence>],
) {
    let mut visitor = SymbolIndexVisitor {
        index,
        aliases,
        pending_callee_reads: BTreeMap::new(),
        argument_matchers,
        argument_evidence,
    };
    program.visit_with(&mut visitor);
}

struct SymbolIndexVisitor<'a, 'rules> {
    index: &'a mut SymbolIndex,
    aliases: &'a AliasInfo,
    pending_callee_reads: BTreeMap<String, u32>,
    argument_matchers: &'rules [(usize, MemberCallMatcher)],
    argument_evidence: &'a mut [Vec<ApiEvidence>],
}

impl SymbolIndexVisitor<'_, '_> {
    fn record_identifier_call(&mut self, ident: &Ident) {
        let name = ident.sym.to_string();
        self.index.increment(ApiMatchKind::Call, name.clone());

        match self.aliases.call_provenance(&name, ident.span) {
            SymbolCallProvenance::Global => {
                *self.index.global_calls.entry(name).or_insert(0) += 1;
            }
            SymbolCallProvenance::ModuleExport { module, export } => {
                *self
                    .index
                    .module_calls
                    .entry((module.clone(), export.clone()))
                    .or_insert(0) += 1;
            }
            SymbolCallProvenance::Local => {}
        }
    }

    fn record_member_call(&mut self, member: &MemberExpr, call: Option<&CallExpr>) {
        let syntactic_chain = member_chain(member);
        let resolved_chain = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.resolve_member_chain(member, chain));
        let module_member = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.member_call_provenance_for_chain(member, chain));

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
                .increment(ApiMatchKind::MemberCall, chain.clone());
            *self.pending_callee_reads.entry(chain.clone()).or_insert(0) += 1;
        }
        if let Some(chain) = resolved_chain {
            let chain = canonical_rooted_chain(&chain).to_string();
            *self
                .index
                .rooted_member_calls
                .entry(chain.clone())
                .or_insert(0) += 1;
        }
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            *self
                .index
                .module_member_calls
                .entry((module.clone(), member.clone()))
                .or_insert(0) += 1;
        }
    }

    fn collect_argument_evidence(
        &mut self,
        call: &CallExpr,
        syntactic_chain: Option<&str>,
        resolved_chain: Option<&str>,
        module_member: Option<&SymbolMemberProvenance>,
    ) {
        for (rule_index, matcher) in self.argument_matchers {
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
                    call.args
                        .get(arg_matcher.index)
                        .and_then(|argument| static_string(&argument.expr))
                        .is_some_and(|value| {
                            arg_matcher.values.is_empty()
                                || arg_matcher.values.iter().any(|expected| expected == &value)
                        })
                })
                && matcher.arg_object_keys.iter().all(|key_matcher| {
                    call.args
                        .get(key_matcher.index)
                        .and_then(|argument| object_literal(&argument.expr))
                        .is_some_and(|object| {
                            key_matcher
                                .keys
                                .iter()
                                .all(|expected| object_has_key(object, expected))
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
                && matcher.assigned_properties.iter().all(|property_matcher| {
                    resolved_chain.or(syntactic_chain).is_some_and(|object| {
                        self.aliases.has_later_static_property_write(
                            canonical_rooted_chain(object),
                            &property_matcher.property,
                            &property_matcher.values,
                            call.span,
                        )
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
                        .increment(ApiMatchKind::Class, format!("{module}.{export}"));
                }
            }
            Expr::Member(member) => {
                if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                    self.aliases.member_call_provenance(member)
                {
                    self.index
                        .increment(ApiMatchKind::Class, format!("{module}.{member}"));
                }
            }
            Expr::Paren(paren) => self.record_module_class_expr(&paren.expr),
            _ => {}
        }
    }
}

impl Visit for SymbolIndexVisitor<'_, '_> {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_string();
        self.index.increment(ApiMatchKind::Import, module);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(module) = require_call_module_name(call) {
            self.index.increment(ApiMatchKind::Import, module);
        }

        match &call.callee {
            Callee::Expr(callee) => match effective_callee_expr(callee) {
                Expr::Ident(ident) => {
                    self.record_identifier_call(ident);
                }
                Expr::Member(member) => {
                    self.record_member_call(member, Some(call));
                }
                _ => {}
            },
            Callee::Super(_) => self.index.increment(ApiMatchKind::Call, "super"),
            Callee::Import(_) => self.index.increment(ApiMatchKind::Call, "import"),
        }

        call.visit_children_with(self);
    }

    fn visit_opt_chain_expr(&mut self, chain: &OptChainExpr) {
        if let OptChainBase::Call(call) = &*chain.base {
            match &*call.callee {
                Expr::Ident(ident) => self.record_identifier_call(ident),
                Expr::Member(member) => self.record_member_call(member, None),
                _ => {
                    if let Some(raw) = expr_name(&call.callee) {
                        self.index.increment(ApiMatchKind::MemberCall, raw);
                    }
                    if let Some(rooted) = self.aliases.rooted_expr_chain(&call.callee) {
                        let rooted = canonical_rooted_chain(&rooted).to_string();
                        *self
                            .index
                            .rooted_member_calls
                            .entry(rooted.clone())
                            .or_insert(0) += 1;
                    }
                    if let Some(member) = expr_member(&call.callee)
                        && let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                            self.aliases.member_call_provenance(member)
                    {
                        *self
                            .index
                            .module_member_calls
                            .entry((module.clone(), member.clone()))
                            .or_insert(0) += 1;
                    }
                }
            }
        }
        chain.visit_children_with(self);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        let syntactic_chain = member_chain(member);
        if let Some(chain) = syntactic_chain.as_ref() {
            if let Some(skip_count) = self.pending_callee_reads.get_mut(chain.as_str()) {
                *skip_count -= 1;
                if *skip_count == 0 {
                    self.pending_callee_reads.remove(chain.as_str());
                }

                member.visit_children_with(self);
                return;
            }

            let chain = canonical_rooted_chain(chain).to_string();
            self.index.increment(ApiMatchKind::MemberRead, chain);
        }
        if let Some(resolved_chain) = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.resolve_member_chain(member, chain))
        {
            self.index.increment(
                ApiMatchKind::MemberRead,
                canonical_rooted_chain(&resolved_chain).to_string(),
            );
        }
        let module_member = syntactic_chain
            .as_deref()
            .and_then(|chain| self.aliases.member_call_provenance_for_chain(member, chain));
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = module_member {
            self.index
                .increment(ApiMatchKind::Class, format!("{module}.{member}"));
        }

        member.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, new_expr: &NewExpr) {
        match &*new_expr.callee {
            Expr::Ident(ident) => {
                match self.aliases.call_provenance(ident.sym.as_ref(), ident.span) {
                    SymbolCallProvenance::Global => {
                        self.index
                            .increment(ApiMatchKind::Constructor, ident.sym.to_string());
                    }
                    SymbolCallProvenance::ModuleExport { module, export } => {
                        self.index
                            .increment(ApiMatchKind::Constructor, export.clone());
                        self.index
                            .increment(ApiMatchKind::Constructor, format!("{module}.{export}"));
                    }
                    SymbolCallProvenance::Local => {}
                }
            }
            Expr::Member(member) => {
                if let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) =
                    self.aliases.member_call_provenance(member)
                {
                    self.index
                        .increment(ApiMatchKind::Constructor, member.clone());
                    self.index
                        .increment(ApiMatchKind::Constructor, format!("{module}.{member}"));
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
            self.index.increment(ApiMatchKind::MemberRead, name);
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
        self.index.increment(ApiMatchKind::StringLiteral, literal);

        value.visit_children_with(self);
    }

    fn visit_tpl(&mut self, template: &Tpl) {
        for quasi in &template.quasis {
            let literal = quasi
                .cooked
                .as_ref()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| quasi.raw.to_string());
            self.index.increment(ApiMatchKind::StringLiteral, literal);
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

fn object_has_key(object: &ObjectLit, expected: &str) -> bool {
    object.props.iter().any(|property| match property {
        PropOrSpread::Prop(property) => match &**property {
            Prop::Shorthand(ident) => ident.sym == *expected,
            Prop::KeyValue(key_value) => prop_name(&key_value.key).as_deref() == Some(expected),
            Prop::Method(method) => prop_name(&method.key).as_deref() == Some(expected),
            Prop::Getter(getter) => prop_name(&getter.key).as_deref() == Some(expected),
            Prop::Setter(setter) => prop_name(&setter.key).as_deref() == Some(expected),
            Prop::Assign(assign) => assign.key.sym == *expected,
        },
        PropOrSpread::Spread(_) => false,
    })
}
