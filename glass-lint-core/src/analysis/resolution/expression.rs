//! Position-sensitive identifier, member, and expression resolution.

use std::sync::Arc;

use glass_lint_datastructures::SymbolPath;
use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::{
    resolution::{
        Callee, ConstValue, Expr, Ident, Lit, MemberExpr, ResolutionKey, ResolvedValue, Resolver,
        SymbolCallProvenance, SymbolMemberProvenance, Value, ValueId, syntax_constant,
    },
    scope::ScopeId,
    syntax::{BudgetComponent, UnknownReason},
    value::MAX_VALUES,
};

impl Resolver<'_> {
    /// Narrow query: return only the interned value ID for an identifier,
    /// avoiding a clone of the full `ResolvedValue` on cache hits.
    pub(in crate::analysis) fn resolve_ident_id(&mut self, ident: &Ident) -> ValueId {
        let key = ResolutionKey::Ident {
            range: ident.span.into(),
            symbol: ident.sym.to_smolstr(),
        };
        if let Some(cached) = self.cache.resolved_values.get(&key) {
            return cached.id;
        }
        self.resolve_ident(ident).id
    }

    /// Narrow query for a member expression when callers need only its arena
    /// identity. Cache hits remain borrowed through the cache's `Arc` rather
    /// than cloning the complete provenance record.
    pub(in crate::analysis) fn resolve_member_id(&mut self, member: &MemberExpr) -> ValueId {
        let key = ResolutionKey::Member {
            range: member.span.into(),
        };
        if let Some(cached) = self.cache.resolved_values.get(&key) {
            return cached.id;
        }
        self.resolve_member(member).id
    }

    /// Narrow expression query used by identity-only fact construction.
    pub(in crate::analysis) fn resolve_expr_id(&mut self, expr: &Expr) -> ValueId {
        match expr {
            Expr::Ident(ident) => self.resolve_ident_id(ident),
            Expr::Member(member) => self.resolve_member_id(member),
            _ => self.resolve_expr(expr).id,
        }
    }

    /// Returns a CommonJS module only when the callee is proven to be the
    /// unshadowed global loader. Import collection and alias provenance both
    /// depend on this conservative distinction.
    pub(in crate::analysis) fn resolve_ident(&mut self, ident: &Ident) -> Arc<ResolvedValue> {
        self.resolve_ident_uncached(ident)
    }

    fn resolve_ident_uncached(&mut self, ident: &Ident) -> Arc<ResolvedValue> {
        let key = ResolutionKey::Ident {
            range: ident.span.into(),
            symbol: ident.sym.to_smolstr(),
        };
        if let Some(value) = self.cache.resolved_values.get(&key) {
            return Arc::clone(value);
        }
        if !self.cache.resolving.insert(key.clone()) {
            return Self::archive_unknown_with_reason(UnknownReason::Cycle);
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
            && !matches!(seed.call, SymbolCallProvenance::Unknown(_))
            && self.value_arena_exhausted()
        {
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: MAX_VALUES,
                observed: None,
            })
        } else {
            self.call_provenance_at(id, rooted_chain.as_ref(), ident.span)
        };
        let id = match &call {
            SymbolCallProvenance::Global { name } => {
                self.values.intern(Value::Global(name.clone()))
            }
            _ => id,
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
        let resolved = Arc::new(ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member: None,
            bound_arguments: seed.bound_arguments,
            syntactic_chain: None,
        });
        self.cache_resolution(&key, resolved.clone());
        resolved
    }

    pub(in crate::analysis) fn scope_at(&self, span: swc_common::Span) -> ScopeId {
        self.scopes.scope_at(span)
    }

    pub(in crate::analysis) fn function_scope_at(
        &self,
        scope: ScopeId,
    ) -> crate::analysis::value::FunctionId {
        self.scopes.function_scope_at(scope)
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

    pub(in crate::analysis) fn resolve_member(
        &mut self,
        member: &MemberExpr,
    ) -> Arc<ResolvedValue> {
        self.resolve_member_uncached(member)
    }

    fn resolve_member_uncached(&mut self, member: &MemberExpr) -> Arc<ResolvedValue> {
        let key = ResolutionKey::Member {
            range: member.span.into(),
        };
        if let Some(value) = self.cache.resolved_values.get(&key) {
            return Arc::clone(value);
        }
        if !self.cache.resolving.insert(key.clone()) {
            return Self::archive_unknown_with_reason(UnknownReason::Cycle);
        }
        let seed = self.scopes.member_value_seed(member);
        let syntactic = seed.syntactic_chain.clone();
        // Prefer the alias-expanded path. Falling back to a rooted member keeps
        // direct global/`this` access available when no local alias is present.
        let rooted_chain = seed
            .rooted_chain
            .and_then(|path| self.scopes.symbol_path(&path));
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
                limit: MAX_VALUES,
                observed: None,
            })
        } else {
            self.call_provenance_at(id, rooted_chain.as_ref(), member.span)
        };
        let id = match &call {
            SymbolCallProvenance::Global { name } => {
                self.values.intern(Value::Global(name.clone()))
            }
            _ => id,
        };
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, .. }) = &module_member {
            self.values.intern(Value::ModuleNamespace(module.clone()));
        }
        let resolved = Arc::new(ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member: seed.returned_member.and_then(|(source, member)| {
                Some((
                    self.scopes.symbol_path(&source)?,
                    self.scopes.symbol_path(&member)?,
                ))
            }),
            bound_arguments: None,
            syntactic_chain: syntactic,
        });
        self.cache_resolution(&key, resolved.clone());
        resolved
    }

    pub(in crate::analysis) fn resolve_expr(&mut self, expr: &Expr) -> Arc<ResolvedValue> {
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
                            self.resolve_expr_id(&element.expr)
                        })
                    })
                    .collect();
                self.static_value(Value::StaticArray(values))
            }
            Expr::Object(_) => {
                let id = self.intern_const_value(syntax_constant::evaluate(expr, self), None);
                Self::archive_local(id)
            }
            Expr::Call(call) => self.resolve_call_expression(call),
            Expr::Await(await_expr) => self.resolve_expr(&await_expr.arg),
            Expr::New(new_expr) => self.fresh_object_value_at(new_expr.span),
            _ => Self::unknown(),
        }
    }

    fn cache_resolution(&mut self, key: &ResolutionKey, value: Arc<ResolvedValue>) {
        self.cache.resolved_values.insert(key.clone(), value);
        self.cache.resolving.remove(key);
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

    pub(in crate::analysis) fn rooted_expr_chain(&mut self, expr: &Expr) -> Option<SymbolPath> {
        match expr {
            Expr::Ident(ident) => self.resolve_ident(ident).rooted_chain.clone().or_else(|| {
                ident
                    .span
                    .is_dummy()
                    .then(|| SymbolPath::from(ident.sym.as_ref()))
            }),
            Expr::Member(member) => self.resolve_member(member).rooted_chain.clone(),
            Expr::Call(call) => match &call.callee {
                Callee::Expr(callee) => self.rooted_expr_chain(callee),
                Callee::Super(_) | Callee::Import(_) => None,
            },
            Expr::OptChain(chain) => match &*chain.base {
                swc_ecma_ast::OptChainBase::Member(member) => {
                    self.resolve_member(member).rooted_chain.clone()
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
        self.cache
            .resolved_values
            .get(&key)
            .and_then(|value| value.syntactic_chain.clone())
            .or_else(|| crate::analysis::syntax::member_expression_chain(member))
    }

    pub(in crate::analysis) fn class_provenance(
        &mut self,
        expr: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        match &self.resolve_expr(expr).call {
            SymbolCallProvenance::ModuleExport { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn unknown() -> Arc<ResolvedValue> {
        Self::archive_unknown_with_reason(UnknownReason::Unresolved)
    }

    fn archive_unknown_with_reason(reason: UnknownReason) -> Arc<ResolvedValue> {
        let mut value = ResolvedValue::local(ValueId::UNKNOWN);
        value.call = SymbolCallProvenance::Unknown(reason);
        Arc::new(value)
    }

    fn archive_local(id: ValueId) -> Arc<ResolvedValue> {
        Arc::new(ResolvedValue::local(id))
    }

    pub(in crate::analysis) fn static_value(&mut self, value: Value) -> Arc<ResolvedValue> {
        let is_unknown = matches!(value, Value::Unknown);
        let id = self.values.intern(value);
        if id == ValueId::UNKNOWN && !is_unknown && self.value_arena_exhausted() {
            return Self::archive_unknown_with_reason(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: MAX_VALUES,
                observed: None,
            });
        }
        Arc::new(ResolvedValue::local(id))
    }

    pub(in crate::analysis) fn fresh_object_value(&mut self) -> Arc<ResolvedValue> {
        let Some(object) = self.values.allocate_object_id() else {
            return Self::unknown();
        };
        self.static_value(Value::Object(object))
    }

    pub(in crate::analysis) fn fresh_object_value_at(
        &mut self,
        span: swc_common::Span,
    ) -> Arc<ResolvedValue> {
        let key = span.into();
        if let Some(value) = self.cache.fresh_values.get(&key).copied() {
            return Arc::new(ResolvedValue::local(value));
        }
        let value = self.fresh_object_value();
        self.cache.fresh_values.insert(key, value.id);
        value
    }
}
