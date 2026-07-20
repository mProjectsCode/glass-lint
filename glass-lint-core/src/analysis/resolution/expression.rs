//! Position-sensitive identifier, member, and expression resolution.

use smol_str::SmolStr;

use super::{
    Callee, ConstValue, Expr, Ident, Lit, MemberExpr, ResolutionKey, ResolvedValue, Resolver,
    SymbolCallProvenance, SymbolMemberProvenance, Value, ValueId, syntax_constant,
};
use crate::analysis::{
    SymbolPath,
    scope::ScopeId,
    syntax::{BudgetComponent, UnknownReason},
};

impl Resolver {
    /// Returns a CommonJS module only when the callee is proven to be the
    /// unshadowed global loader. Import collection and alias provenance both
    /// depend on this conservative distinction.
    pub(in crate::analysis) fn resolve_ident(&self, ident: &Ident) -> ResolvedValue {
        self.resolve_ident_uncached(ident)
    }

    fn resolve_ident_uncached(&self, ident: &Ident) -> ResolvedValue {
        let key = ResolutionKey::Ident {
            range: ident.span.into(),
            symbol: ident.sym.to_string(),
        };
        if let Some(value) = self.state.borrow().resolved_values.get(&key).cloned() {
            return value;
        }
        if !self.state.borrow_mut().resolving.insert(key.clone()) {
            return Self::unknown_with_reason(UnknownReason::Cycle);
        }
        let seed = self.scopes.ident_value_seed(ident);
        let rooted_chain = seed.rooted_chain;
        let id = match seed.constant {
            ConstValue::Unknown => {
                self.intern_call_value(&seed.call, rooted_chain.as_ref(), seed.binding)
            }
            value => self.intern_const_value(value, seed.binding),
        };
        let call = if id == ValueId::UNKNOWN
            && !matches!(
                seed.call,
                SymbolCallProvenance::Unknown(_) | SymbolCallProvenance::Ambiguous
            )
            && self.value_arena_exhausted()
        {
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: super::MAX_VALUES,
                observed: None,
            })
        } else {
            self.call_provenance_at(id, rooted_chain.as_ref(), ident.span)
        };
        let module_member = match &call {
            SymbolCallProvenance::ModuleExport { module, export } => {
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: module.clone(),
                    member: export.clone(),
                })
            }
            _ => None,
        };
        let resolved = ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member: None,
            bound_arguments: seed.bound_arguments,
            syntactic_chain: None,
        };
        self.cache_resolution(&key, resolved.clone());
        resolved
    }

    pub(in crate::analysis) fn scope_at(&self, span: swc_common::Span) -> ScopeId {
        self.scopes.scope_at(span)
    }

    pub(in crate::analysis) fn function_id_for_scope(
        &self,
        scope: ScopeId,
    ) -> crate::analysis::value::FunctionId {
        self.scopes.function_id_for_scope(scope)
    }

    pub(in crate::analysis) fn function_id_for_expr(
        &self,
        expr: &Expr,
    ) -> Option<crate::analysis::value::FunctionId> {
        self.scopes.function_id_for_expr(expr)
    }

    pub(in crate::analysis) fn function_id_for_name(
        &self,
        name: &str,
        span: swc_common::Span,
    ) -> Option<crate::analysis::value::FunctionId> {
        self.scopes.function_binding_at(name, span)
    }

    pub(in crate::analysis) fn function_id_for_span(
        &self,
        span: swc_common::Span,
    ) -> Option<crate::analysis::value::FunctionId> {
        self.scopes.function_id_for_span(span)
    }

    pub(in crate::analysis) fn resolve_member(&self, member: &MemberExpr) -> ResolvedValue {
        self.resolve_member_uncached(member)
    }

    fn resolve_member_uncached(&self, member: &MemberExpr) -> ResolvedValue {
        let key = ResolutionKey::Member {
            range: member.span.into(),
        };
        if let Some(value) = self.state.borrow().resolved_values.get(&key).cloned() {
            return value;
        }
        if !self.state.borrow_mut().resolving.insert(key.clone()) {
            return Self::unknown_with_reason(UnknownReason::Cycle);
        }
        let seed = self.scopes.member_value_seed(member);
        let syntactic = seed.syntactic_chain;
        // Prefer the alias-expanded path. Falling back to a rooted member keeps
        // direct global/`this` access available when no local alias is present.
        let rooted_chain = seed.rooted_chain;
        let module_member = seed.module_member;
        let scoped_call = match &module_member {
            Some(SymbolMemberProvenance::ModuleNamespace { module, member }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: member.clone(),
                }
            }
            None => SymbolCallProvenance::Local,
        };
        let id = self.intern_call_value(&scoped_call, rooted_chain.as_ref(), seed.binding);
        let call = if id == ValueId::UNKNOWN && self.value_arena_exhausted() {
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: super::MAX_VALUES,
                observed: None,
            })
        } else {
            self.call_provenance_at(id, rooted_chain.as_ref(), member.span)
        };
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, .. }) = &module_member {
            self.state
                .borrow_mut()
                .values
                .intern(Value::ModuleNamespace(module.clone()));
        }
        let resolved = ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member: seed.returned_member,
            bound_arguments: None,
            syntactic_chain: syntactic,
        };
        self.cache_resolution(&key, resolved.clone());
        resolved
    }

    pub(in crate::analysis) fn resolve_expr(&self, expr: &Expr) -> ResolvedValue {
        match expr {
            Expr::Ident(ident) => self.resolve_ident(ident),
            Expr::Member(member) => self.resolve_member(member),
            Expr::Paren(paren) => self.resolve_expr(&paren.expr),
            Expr::Assign(assignment) => match &assignment.left {
                swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(
                    ident,
                )) => self.resolve_ident(&ident.id),
                _ => self.resolve_expr(&assignment.right),
            },
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .map_or_else(Self::unknown, |last| self.resolve_expr(last)),
            Expr::Lit(Lit::Str(value)) => self.static_value(Value::StaticString(
                value.value.to_string_lossy().to_string(),
            )),
            Expr::Lit(Lit::Num(value)) => syntax_constant::non_negative_integer(value.value)
                .map_or_else(Self::unknown, |value| {
                    self.static_value(Value::StaticNumber(value))
                }),
            Expr::Array(array) => {
                let values = array
                    .elems
                    .iter()
                    .map(|element| {
                        element.as_ref().map_or(ValueId::UNKNOWN, |element| {
                            self.resolve_expr(&element.expr).id
                        })
                    })
                    .collect();
                self.static_value(Value::StaticArray(values))
            }
            Expr::Object(_) => {
                // Preserve finite object structure for helper parameter
                // projection. The constant evaluator already applies the
                // depth, node, spread, and mutation bounds used elsewhere.
                let id = self.intern_const_value(syntax_constant::evaluate(expr, self), None);
                ResolvedValue::local(id)
            }
            Expr::Call(call) => self.resolve_call_expression(call),
            Expr::Await(await_expr) => self.resolve_expr(&await_expr.arg),
            Expr::New(new_expr) => self.fresh_object_value_at(new_expr.span),
            _ => Self::unknown(),
        }
    }

    fn cache_resolution(&self, key: &ResolutionKey, value: ResolvedValue) {
        self.state
            .borrow_mut()
            .resolved_values
            .insert(key.clone(), value);
        self.state.borrow_mut().resolving.remove(key);
    }

    pub(in crate::analysis) fn static_string_expr(&self, expr: &Expr) -> Option<String> {
        let value = syntax_constant::evaluate(expr, self).string()?.to_string();
        self.state
            .borrow_mut()
            .values
            .intern(Value::StaticString(value.clone()));
        Some(value)
    }

    pub(in crate::analysis) fn static_string_array_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match syntax_constant::evaluate(expr, self) {
            ConstValue::Array(values) => values
                .into_iter()
                .map(|value| value.string().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }

    pub(in crate::analysis) fn object_keys_expr(&self, expr: &Expr) -> Option<Vec<SmolStr>> {
        let keys = syntax_constant::evaluate(expr, self).object_keys()?;
        let unknown = ValueId::UNKNOWN;
        let mut state = self.state.borrow_mut();
        self.names.with_mut(|names| {
            state
                .values
                .intern_static_object(keys.iter().cloned().map(|key| (key, unknown)), names);
        });
        Some(keys)
    }

    pub(in crate::analysis) fn rooted_expr_chain(&self, expr: &Expr) -> Option<SymbolPath> {
        match expr {
            Expr::Ident(ident) => self.resolve_ident(ident).rooted_chain.or_else(|| {
                ident
                    .span
                    .is_dummy()
                    .then(|| SymbolPath::from(ident.sym.as_ref()))
            }),
            Expr::Member(member) => self.resolve_member(member).rooted_chain,
            Expr::Call(call) => match &call.callee {
                Callee::Expr(callee) => self.rooted_expr_chain(callee),
                Callee::Super(_) | Callee::Import(_) => None,
            },
            Expr::OptChain(chain) => match &*chain.base {
                swc_ecma_ast::OptChainBase::Member(member) => {
                    self.resolve_member(member).rooted_chain
                }
                swc_ecma_ast::OptChainBase::Call(call) => self.rooted_expr_chain(&call.callee),
            },
            Expr::Paren(paren) => self.rooted_expr_chain(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.rooted_expr_chain(expr)),
            Expr::TsAs(value) => self.rooted_expr_chain(&value.expr),
            Expr::TsNonNull(value) => self.rooted_expr_chain(&value.expr),
            Expr::TsSatisfies(value) => self.rooted_expr_chain(&value.expr),
            Expr::TsTypeAssertion(value) => self.rooted_expr_chain(&value.expr),
            _ => None,
        }
    }

    pub(in crate::analysis) fn member_expression_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let key = ResolutionKey::Member {
            range: member.span.into(),
        };
        self.state
            .borrow()
            .resolved_values
            .get(&key)
            .and_then(|value| value.syntactic_chain.clone())
            .or_else(|| crate::analysis::syntax::member_expression_chain(member))
    }

    pub(in crate::analysis) fn class_provenance(&self, expr: &Expr) -> Option<(SmolStr, SmolStr)> {
        match self.resolve_expr(expr).call {
            SymbolCallProvenance::ModuleExport { module, export } => Some((module, export)),
            _ => None,
        }
    }

    pub(in crate::analysis) fn unknown() -> ResolvedValue {
        Self::unknown_with_reason(UnknownReason::Unresolved)
    }

    pub(in crate::analysis) fn unknown_with_reason(reason: UnknownReason) -> ResolvedValue {
        let mut value = ResolvedValue::local(ValueId::UNKNOWN);
        value.call = SymbolCallProvenance::Unknown(reason);
        value
    }

    pub(in crate::analysis) fn static_value(&self, value: Value) -> ResolvedValue {
        let is_unknown = matches!(value, Value::Unknown);
        let id = self.state.borrow_mut().values.intern(value);
        if id == ValueId::UNKNOWN && !is_unknown && self.value_arena_exhausted() {
            return Self::unknown_with_reason(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: super::MAX_VALUES,
                observed: None,
            });
        }
        ResolvedValue::local(id)
    }

    pub(in crate::analysis) fn fresh_object_value(&self) -> ResolvedValue {
        let Some(object) = self.state.borrow_mut().values.allocate_object_id() else {
            return Self::unknown();
        };
        self.static_value(Value::Object(object))
    }

    pub(in crate::analysis) fn fresh_object_value_at(
        &self,
        span: swc_common::Span,
    ) -> ResolvedValue {
        let key = span.into();
        if let Some(value) = self.state.borrow().fresh_values.get(&key).copied() {
            return ResolvedValue::local(value);
        }
        let value = self.fresh_object_value();
        self.state.borrow_mut().fresh_values.insert(key, value.id);
        value
    }
}
