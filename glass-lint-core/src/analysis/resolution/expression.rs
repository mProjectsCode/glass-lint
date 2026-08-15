//! Position-sensitive identifier, member, and expression resolution.

use glass_lint_datastructures::SymbolPath;
use smol_str::ToSmolStr;

use crate::analysis::{
    model::{scope::FunctionId, value::MAX_VALUES},
    resolution::{
        ConstValue, Expr, Ident, Lit, MemberExpr, ResolutionKey, ResolutionProvenance,
        ResolvedValue, Resolver, SymbolCallProvenance, SymbolMemberProvenance, Value, ValueId,
        syntax_constant,
    },
    scope::ScopeId,
    syntax::UnknownReason,
};

struct ResolutionSeed {
    provisional_id: ValueId,
    provenance: ResolutionProvenance,
}

enum ResolutionStart {
    Cached(ResolvedValue),
    Cycle,
    Active(ResolutionGuard),
}

struct ResolutionGuard {
    key: ResolutionKey,
}

impl ResolutionGuard {
    fn commit(self, cache: &mut super::ResolverCache, value: ResolvedValue) -> ResolvedValue {
        cache
            .resolved_values
            .insert(self.key.clone(), value.clone());
        cache.resolving.remove(&self.key);
        value
    }
}

impl ResolutionSeed {
    fn into_resolved(
        self,
        final_id: ValueId,
        call: SymbolCallProvenance,
        module_member: Option<SymbolMemberProvenance>,
    ) -> ResolvedValue {
        let Self { provenance, .. } = self;
        let ResolutionProvenance {
            rooted_chain,
            returned_member,
            bound_arguments,
            syntactic_chain,
            ..
        } = provenance;
        ResolvedValue::with_provenance(
            final_id,
            ResolutionProvenance {
                rooted_chain,
                call,
                module_member,
                returned_member,
                bound_arguments,
                syntactic_chain,
            },
        )
    }
}

impl Resolver<'_> {
    pub(in crate::analysis) fn resolve_string_literal(
        &mut self,
        value: &swc_ecma_ast::Str,
    ) -> ResolvedValue {
        self.static_string(value.value.to_string_lossy().to_string())
    }

    pub(in crate::analysis) fn resolve_template(
        &mut self,
        template: &swc_ecma_ast::Tpl,
    ) -> ResolvedValue {
        let id = self.intern_const_value(syntax_constant::evaluate(template, self), None);
        ResolvedValue::local(id)
    }

    fn ident_key(ident: &Ident) -> ResolutionKey {
        ResolutionKey::Ident {
            range: ident.span.into(),
            // PERF: Authored spans uniquely identify tokens. Retain spelling
            // only for synthetic identifiers, whose dummy spans can collide;
            // allocating a SmolStr for every cache probe dominated hot bundles.
            synthetic_symbol: ident.span.is_dummy().then(|| ident.sym.to_smolstr()),
        }
    }

    fn member_key(member: &MemberExpr) -> ResolutionKey {
        ResolutionKey::Member {
            range: member.span.into(),
        }
    }

    fn cached_id(&self, key: &ResolutionKey) -> Option<ValueId> {
        self.cache.resolved_values.get(key).map(|cached| cached.id)
    }

    /// Narrow query: return only the interned value ID for an identifier,
    /// avoiding a clone of the full `ResolvedValue` on cache hits.
    pub(in crate::analysis) fn resolve_ident_id(&mut self, ident: &Ident) -> ValueId {
        let key = Self::ident_key(ident);
        if let Some(id) = self.cached_id(&key) {
            return id;
        }
        self.resolve_ident(ident).id
    }

    /// Narrow query for a member expression when callers need only its arena
    /// identity. Cache hits read the identity directly without cloning the
    /// complete provenance record.
    pub(in crate::analysis) fn resolve_member_id(&mut self, member: &MemberExpr) -> ValueId {
        let key = Self::member_key(member);
        if let Some(id) = self.cached_id(&key) {
            return id;
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

    /// Resolve an identifier while preserving its position-sensitive
    /// provenance and cached arena identity.
    pub(in crate::analysis) fn resolve_ident(&mut self, ident: &Ident) -> ResolvedValue {
        let key = Self::ident_key(ident);
        self.resolve_seed(&key, ident.span, |resolver| {
            let seed = resolver.scopes.ident_value_seed(ident);
            let rooted_chain = seed.rooted_chain;
            let id = match seed.constant {
                ConstValue::Unknown => {
                    resolver.intern_call_value(&seed.call, rooted_chain.as_ref(), seed.binding)
                }
                value => resolver.intern_const_value(value, seed.binding),
            };
            ResolutionSeed {
                provisional_id: id,
                provenance: ResolutionProvenance {
                    rooted_chain,
                    call: seed.call,
                    module_member: None,
                    returned_member: None,
                    bound_arguments: seed.bound_arguments,
                    syntactic_chain: None,
                },
            }
        })
    }

    pub(in crate::analysis) fn scope_at(&self, span: swc_common::Span) -> Option<ScopeId> {
        self.scopes.scope_at(span)
    }

    pub(in crate::analysis) fn function_scope_at(&self, scope: ScopeId) -> FunctionId {
        self.scopes.function_scope_at(scope)
    }

    pub(in crate::analysis) fn function_id_for_expr(&self, expr: &Expr) -> Option<FunctionId> {
        self.scopes.function_id_for_expr(expr)
    }

    pub(in crate::analysis) fn function_id_for_name(
        &self,
        name: &str,
        span: swc_common::Span,
    ) -> Option<FunctionId> {
        self.scopes.function_binding_at(name, span)
    }

    pub(in crate::analysis) fn function_id_for_span(
        &self,
        span: swc_common::Span,
    ) -> Option<FunctionId> {
        self.scopes.function_id_for_span(span)
    }

    pub(in crate::analysis) fn rooted_write_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        self.scopes.rooted_write_member_chain(member)
    }

    /// Resolve a member expression while preserving its position-sensitive
    /// provenance and cached arena identity.
    pub(in crate::analysis) fn resolve_member(&mut self, member: &MemberExpr) -> ResolvedValue {
        let key = Self::member_key(member);
        self.resolve_seed(&key, member.span, |resolver| {
            let seed = resolver.scopes.member_value_seed(member);
            let syntactic_chain = seed.syntactic_chain.clone();
            // Prefer the alias-expanded path. Falling back to a rooted member keeps
            // direct global/`this` access available when no local alias is present.
            let rooted_chain = seed
                .rooted_chain
                .clone()
                .and_then(|path| resolver.scopes.symbol_path(&path));
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
            let id = resolver.intern_call_value(&scoped_call, rooted_chain.as_ref(), seed.binding);
            let returned_member = seed.returned_member.and_then(|(source, member)| {
                Some((
                    resolver.scopes.symbol_path(&source)?,
                    resolver.scopes.symbol_path(&member)?,
                ))
            });
            ResolutionSeed {
                provisional_id: id,
                provenance: ResolutionProvenance {
                    rooted_chain,
                    call: scoped_call,
                    module_member,
                    returned_member,
                    bound_arguments: None,
                    syntactic_chain,
                },
            }
        })
    }

    pub(in crate::analysis) fn resolve_expr(&mut self, expr: &Expr) -> ResolvedValue {
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
            Expr::Lit(Lit::Str(value)) => self.resolve_string_literal(value),
            Expr::Lit(Lit::Num(value)) => syntax_constant::non_negative_integer(value.value)
                .map_or_else(Self::unknown, |value| self.static_number(value)),
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
                self.static_array(values)
            }
            Expr::Object(_) | Expr::Bin(_) => {
                let id = self.intern_const_value(syntax_constant::evaluate(expr, self), None);
                ResolvedValue::local(id)
            }
            Expr::Call(call) => self.resolve_call_expression(call),
            Expr::Await(await_expr) => self.resolve_expr(&await_expr.arg),
            Expr::TsAs(value) => self.resolve_expr(&value.expr),
            Expr::TsNonNull(value) => self.resolve_expr(&value.expr),
            Expr::TsSatisfies(value) => self.resolve_expr(&value.expr),
            Expr::TsTypeAssertion(value) => self.resolve_expr(&value.expr),
            Expr::New(new_expr) => self.fresh_object_value_at(new_expr.span),
            _ => Self::unknown(),
        }
    }

    pub(in crate::analysis) fn resolve_binary(
        &mut self,
        binary: &swc_ecma_ast::BinExpr,
    ) -> ResolvedValue {
        let value = syntax_constant::evaluate(binary, self);
        let id = self.intern_const_value(value, None);
        ResolvedValue::local(id)
    }

    fn resolve_seed<F>(
        &mut self,
        key: &ResolutionKey,
        span: swc_common::Span,
        build: F,
    ) -> ResolvedValue
    where
        F: FnOnce(&mut Self) -> ResolutionSeed,
    {
        let start = self.start_resolution(key);
        let ResolutionStart::Active(guard) = start else {
            return match start {
                ResolutionStart::Cached(value) => value,
                ResolutionStart::Cycle => Self::archive_unknown_with_reason(UnknownReason::Cycle),
                ResolutionStart::Active(_) => unreachable!("active resolution handled above"),
            };
        };
        let seed = build(self);
        let resolved = self.finalize_seed(seed, span);
        guard.commit(&mut self.cache, resolved)
    }

    fn start_resolution(&mut self, key: &ResolutionKey) -> ResolutionStart {
        if let Some(value) = self.cache.resolved_values.get(key) {
            return ResolutionStart::Cached(value.clone());
        }
        if !self.cache.resolving.insert(key.clone()) {
            return ResolutionStart::Cycle;
        }
        ResolutionStart::Active(ResolutionGuard { key: key.clone() })
    }

    fn finalize_seed(&mut self, seed: ResolutionSeed, span: swc_common::Span) -> ResolvedValue {
        let call = if seed.provisional_id == ValueId::UNKNOWN
            && !matches!(seed.provenance.call, SymbolCallProvenance::Unknown(_))
            && self.value_arena_exhausted()
        {
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted { limit: MAX_VALUES })
        } else {
            self.call_provenance_at(
                seed.provisional_id,
                seed.provenance.rooted_chain.as_ref(),
                span,
            )
        };
        let final_id = match &call {
            SymbolCallProvenance::Global { name } => self
                .values
                .intern_value_with_binding(Value::Global(name.clone()), None),
            _ => seed.provisional_id,
        };
        let module_member = seed
            .provenance
            .module_member
            .clone()
            .or_else(|| match &call {
                SymbolCallProvenance::ModuleExport { module, export } => {
                    Some(SymbolMemberProvenance::ModuleNamespace {
                        module: module.clone(),
                        member: export.clone(),
                    })
                }
                _ => None,
            });
        seed.into_resolved(final_id, call, module_member)
    }
}

mod static_values;
