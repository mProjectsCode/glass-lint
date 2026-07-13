//! Position-sensitive expression resolution.
//!
//! The lexical fact builder supplies declarations and historical assignments.
//! `Resolver` is the single adapter from those low-level facts to the versioned
//! values consumed by matchers, so callers never make matching decisions from
//! raw identifier spelling.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use super::{
    ast::{SymbolCallProvenance, SymbolMemberProvenance},
    constant::{self, ConstValue, EvalState, Lookup},
    scope::ScopeGraph,
    value::{BindingKey, Value, ValueArena, ValueId},
};
use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr, Program};

#[derive(Debug, Clone)]
pub(super) struct ResolvedValue {
    /// The interned abstract value. `UNKNOWN` is reserved for expressions the
    /// resolver cannot describe precisely enough to match.
    pub(super) id: ValueId,
    /// Canonical rooted spelling, when the value can be followed safely.
    pub(super) rooted_chain: Option<String>,
    /// Callable provenance used by global and module-export call matchers.
    pub(super) call: SymbolCallProvenance,
    /// Namespace provenance for member matchers, retained independently from
    /// `call` because a namespace member can also be read without being called.
    pub(super) module_member: Option<SymbolMemberProvenance>,
    pub(super) returned_member: Option<(String, String)>,
    pub(super) bound_arguments: Option<Vec<Option<super::scope::BoundArgument>>>,
    pub(super) syntactic_chain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ResolutionKey {
    Ident { lo: u32, hi: u32, symbol: String },
    Member { lo: u32, hi: u32 },
}

#[derive(Debug)]
pub(super) struct Resolver {
    scopes: ScopeGraph,
    values: RefCell<ValueArena>,
    fresh_values: RefCell<BTreeMap<(u32, u32), ValueId>>,
    resolved_values: RefCell<BTreeMap<ResolutionKey, ResolvedValue>>,
    resolving: RefCell<BTreeSet<ResolutionKey>>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            scopes: ScopeGraph::default(),
            values: RefCell::new(ValueArena::default()),
            fresh_values: RefCell::new(BTreeMap::new()),
            resolved_values: RefCell::new(BTreeMap::new()),
            resolving: RefCell::new(BTreeSet::new()),
        }
    }
}

impl Resolver {
    pub(super) fn collect(program: &Program) -> Self {
        let scopes = ScopeGraph::collect(program);
        Self {
            scopes,
            values: RefCell::new(ValueArena::default()),
            fresh_values: RefCell::new(BTreeMap::new()),
            resolved_values: RefCell::new(BTreeMap::new()),
            resolving: RefCell::new(BTreeSet::new()),
        }
    }

    /// Returns a CommonJS module only when the callee is proven to be the
    /// unshadowed global loader. Import collection and alias provenance both
    /// depend on this conservative distinction.
    pub(super) fn require_module_name(&self, call: &CallExpr) -> Option<String> {
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Ident(ident) = &**callee else {
            return None;
        };
        if !matches!(
            self.resolve_ident(ident).call,
            SymbolCallProvenance::Global { ref name } if name == "require"
        ) {
            return None;
        }
        let argument = call.args.first()?;
        let Expr::Lit(Lit::Str(module)) = &*argument.expr else {
            return None;
        };
        Some(module.value.to_string_lossy().to_string())
    }

    pub(super) fn resolve_ident(&self, ident: &Ident) -> ResolvedValue {
        self.resolve_ident_uncached(ident)
    }

    fn resolve_ident_uncached(&self, ident: &Ident) -> ResolvedValue {
        let key = ResolutionKey::Ident {
            lo: ident.span.lo.0,
            hi: ident.span.hi.0,
            symbol: ident.sym.to_string(),
        };
        if let Some(value) = self.resolved_values.borrow().get(&key).cloned() {
            return value;
        }
        if !self.resolving.borrow_mut().insert(key.clone()) {
            return self.unknown();
        }
        let seed = self.scopes.ident_value_seed(ident);
        let rooted_chain = seed.rooted_chain.map(|chain| chain.to_string());
        let id = match seed.constant {
            ConstValue::Unknown => {
                self.intern_call_value(&seed.call, rooted_chain.as_deref(), seed.binding)
            }
            value => self.intern_const_value(value, seed.binding),
        };
        let call = self.call_provenance_for_value(id);
        let module_member = match &call {
            SymbolCallProvenance::ModuleExport { module, export } => {
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: module.clone(),
                    member: export.clone(),
                })
            }
            _ => None,
        };
        let returned_member = None;
        let resolved = ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member,
            bound_arguments: seed.bound_arguments,
            syntactic_chain: None,
        };
        self.resolved_values
            .borrow_mut()
            .insert(key, resolved.clone());
        self.resolving.borrow_mut().remove(&ResolutionKey::Ident {
            lo: ident.span.lo.0,
            hi: ident.span.hi.0,
            symbol: ident.sym.to_string(),
        });
        resolved
    }

    pub(super) fn scope_chain_at(&self, span: swc_common::Span) -> Vec<usize> {
        self.scopes.scope_chain_at(span)
    }

    pub(super) fn function_id_for_scope(&self, scope: usize) -> super::value::FunctionId {
        self.scopes.function_id_for_scope(scope)
    }

    pub(super) fn function_id_for_expr(&self, expr: &Expr) -> Option<super::value::FunctionId> {
        self.scopes.function_id_for_expr(expr)
    }

    pub(super) fn resolve_member(&self, member: &MemberExpr) -> ResolvedValue {
        self.resolve_member_uncached(member)
    }

    fn resolve_member_uncached(&self, member: &MemberExpr) -> ResolvedValue {
        let key = ResolutionKey::Member {
            lo: member.span.lo.0,
            hi: member.span.hi.0,
        };
        if let Some(value) = self.resolved_values.borrow().get(&key).cloned() {
            return value;
        }
        if !self.resolving.borrow_mut().insert(key.clone()) {
            return self.unknown();
        }
        let seed = self.scopes.member_value_seed(member);
        let syntactic = seed.syntactic_chain.map(|chain| chain.to_string());
        // Prefer the alias-expanded path. Falling back to a rooted member keeps
        // direct global/`this` access available when no local alias is present.
        let rooted_chain = seed.rooted_chain.map(|chain| chain.to_string());
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
        let id = self.intern_call_value(&scoped_call, rooted_chain.as_deref(), seed.binding);
        let call = self.call_provenance_for_value(id);
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, .. }) = &module_member {
            self.values
                .borrow_mut()
                .intern(Value::ModuleNamespace(module.clone()));
        }
        let resolved = ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member: seed
                .returned_member
                .map(|(source, member)| (source.to_string(), member)),
            bound_arguments: None,
            syntactic_chain: syntactic,
        };
        self.resolved_values
            .borrow_mut()
            .insert(key, resolved.clone());
        self.resolving.borrow_mut().remove(&ResolutionKey::Member {
            lo: member.span.lo.0,
            hi: member.span.hi.0,
        });
        resolved
    }

    pub(super) fn resolve_expr(&self, expr: &Expr) -> ResolvedValue {
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
                .map_or_else(|| self.unknown(), |last| self.resolve_expr(last)),
            Expr::Lit(Lit::Str(value)) => self.static_value(Value::StaticString(
                value.value.to_string_lossy().to_string(),
            )),
            Expr::Lit(Lit::Num(value))
                if value.value.is_finite()
                    && value.value >= 0.0
                    && value.value.fract() == 0.0
                    && value.value <= usize::MAX as f64 =>
            {
                self.static_value(Value::StaticNumber(value.value as usize))
            }
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
                let id = self.intern_const_value(constant::evaluate(expr, self), None);
                ResolvedValue {
                    id,
                    rooted_chain: None,
                    call: SymbolCallProvenance::Local,
                    module_member: None,
                    returned_member: None,
                    bound_arguments: None,
                    syntactic_chain: None,
                }
            }
            Expr::Call(call) => self.resolve_call_expression(call),
            Expr::New(new_expr) => self.fresh_object_value_at(new_expr.span),
            _ => self.unknown(),
        }
    }

    pub(super) fn static_string_expr(&self, expr: &Expr) -> Option<String> {
        let value = constant::evaluate(expr, self).string()?.to_string();
        self.values
            .borrow_mut()
            .intern(Value::StaticString(value.clone()));
        Some(value)
    }

    pub(super) fn static_string_array_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match constant::evaluate(expr, self) {
            ConstValue::Array(values) => values
                .into_iter()
                .map(|value| value.string().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }

    pub(super) fn object_keys_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        let keys = constant::evaluate(expr, self).object_keys()?;
        let mut values = self.values.borrow_mut();
        let unknown = ValueId::UNKNOWN;
        values.intern(Value::StaticObject(
            keys.iter().cloned().map(|key| (key, unknown)).collect(),
        ));
        Some(keys)
    }

    pub(super) fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Ident(ident) => self
                .resolve_ident(ident)
                .rooted_chain
                .or_else(|| ident.span.is_dummy().then(|| ident.sym.to_string())),
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

    pub(super) fn member_chain(&self, member: &MemberExpr) -> Option<String> {
        let key = ResolutionKey::Member {
            lo: member.span.lo.0,
            hi: member.span.hi.0,
        };
        self.resolved_values
            .borrow()
            .get(&key)
            .and_then(|value| value.syntactic_chain.clone())
            .or_else(|| super::ast::member_chain(member))
    }

    pub(super) fn class_provenance(&self, expr: &Expr) -> Option<(String, String)> {
        match self.resolve_expr(expr).call {
            SymbolCallProvenance::ModuleExport { module, export } => Some((module, export)),
            _ => None,
        }
    }

    fn unknown(&self) -> ResolvedValue {
        ResolvedValue {
            id: ValueId::UNKNOWN,
            rooted_chain: None,
            call: SymbolCallProvenance::Local,
            module_member: None,
            returned_member: None,
            bound_arguments: None,
            syntactic_chain: None,
        }
    }

    fn static_value(&self, value: Value) -> ResolvedValue {
        let id = self.values.borrow_mut().intern(value);
        ResolvedValue {
            id,
            rooted_chain: None,
            call: SymbolCallProvenance::Local,
            module_member: None,
            returned_member: None,
            bound_arguments: None,
            syntactic_chain: None,
        }
    }

    fn resolve_call_expression(&self, call: &swc_ecma_ast::CallExpr) -> ResolvedValue {
        // Calls normally produce fresh, opaque values. `.bind` is the modeled
        // exception because it preserves a callable's target and arguments.
        let Callee::Expr(callee) = &call.callee else {
            return self.unknown();
        };
        let Expr::Member(member) = &**callee else {
            return self.fresh_object_value_at(call.span);
        };
        if super::ast::member_prop_name(&member.prop).as_deref() != Some("bind") {
            return self.fresh_object_value_at(call.span);
        }
        let target = self.resolve_expr(&member.obj).id;
        let receiver = call
            .args
            .first()
            .map(|argument| self.resolve_expr(&argument.expr).id);
        let bound_arguments = call
            .args
            .iter()
            .skip(1)
            .map(|argument| self.resolve_expr(&argument.expr).id)
            .collect();
        self.static_value(Value::Callable(super::value::CallableValue {
            target,
            receiver,
            bound_arguments,
        }))
    }

    fn intern_call_value(
        &self,
        call: &SymbolCallProvenance,
        rooted: Option<&str>,
        binding: Option<super::value::BindingKey>,
    ) -> ValueId {
        let value = match call {
            SymbolCallProvenance::Global { name } => Value::Global(name.clone()),
            SymbolCallProvenance::ModuleExport { module, export } => Value::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            SymbolCallProvenance::Local => rooted.map_or(Value::Local, rooted_value),
        };
        let mut arena = self.values.borrow_mut();
        let target = arena.intern(value);
        let id = binding.map_or(target, |key| arena.intern(Value::Binding { key, target }));
        drop(arena);
        debug_assert!(self.values.borrow().get(id).is_some());
        id
    }

    /// Convert the canonical value back into matcher provenance. This keeps
    /// the arena authoritative for call identity: scope collection supplies a
    /// typed seed once, but matchers never consume a separately reconstructed
    /// spelling. Unknown or exhausted values are local and therefore fail
    /// closed for strict global/module matchers.
    fn call_provenance_for_value(&self, id: ValueId) -> SymbolCallProvenance {
        let Some(value) = self.values.borrow().get(id).cloned() else {
            return SymbolCallProvenance::Local;
        };
        match value {
            Value::Binding { target, .. } => self.call_provenance_for_value(target),
            Value::Global(name) => SymbolCallProvenance::Global { name },
            Value::ModuleExport { module, export } => {
                SymbolCallProvenance::ModuleExport { module, export }
            }
            Value::Callable(callable) => self.call_provenance_for_value(callable.target),
            Value::RootedMember { root, path } if path.is_empty() => {
                SymbolCallProvenance::Global { name: root }
            }
            _ => SymbolCallProvenance::Local,
        }
    }

    fn const_value(&self, id: ValueId) -> ConstValue {
        let Some(value) = self.values.borrow().get(id).cloned() else {
            return ConstValue::Unknown;
        };
        match value {
            Value::Binding { target, .. } => self.const_value(target),
            Value::StaticString(value) => ConstValue::String(value),
            Value::StaticNumber(value) => ConstValue::NonNegativeInteger(value),
            Value::StaticArray(values) => {
                ConstValue::Array(values.into_iter().map(|id| self.const_value(id)).collect())
            }
            Value::StaticObject(values) => ConstValue::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, self.const_value(value)))
                    .collect(),
            ),
            _ => ConstValue::Unknown,
        }
    }

    fn intern_const_value(&self, value: ConstValue, binding: Option<BindingKey>) -> ValueId {
        let value = match value {
            ConstValue::Unknown => Value::Unknown,
            ConstValue::String(value) => Value::StaticString(value),
            ConstValue::NonNegativeInteger(value) => Value::StaticNumber(value),
            ConstValue::Array(values) => Value::StaticArray(
                values
                    .into_iter()
                    .map(|value| self.intern_const_value(value, None))
                    .collect(),
            ),
            ConstValue::Object(values) => Value::StaticObject(
                values
                    .into_iter()
                    .map(|(key, value)| (key, self.intern_const_value(value, None)))
                    .collect(),
            ),
        };
        let mut arena = self.values.borrow_mut();
        let target = arena.intern(value);
        binding.map_or(target, |key| arena.intern(Value::Binding { key, target }))
    }

    pub(super) fn fresh_object_value(&self) -> ResolvedValue {
        let Some(object) = self.values.borrow_mut().allocate_object_id() else {
            return self.unknown();
        };
        self.static_value(Value::Object(object))
    }

    pub(super) fn fresh_object_value_at(&self, span: swc_common::Span) -> ResolvedValue {
        let key = (span.lo.0, span.hi.0);
        if let Some(value) = self.fresh_values.borrow().get(&key).copied() {
            return ResolvedValue {
                id: value,
                rooted_chain: None,
                call: SymbolCallProvenance::Local,
                module_member: None,
                returned_member: None,
                bound_arguments: None,
                syntactic_chain: None,
            };
        }
        let value = self.fresh_object_value();
        self.fresh_values.borrow_mut().insert(key, value.id);
        value
    }
}

impl Lookup for Resolver {
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        self.const_value(self.resolve_ident(ident).id)
    }

    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.scopes.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue {
        if let Some(property) = constant::property_name_with_state(&member.prop, self, state) {
            return match state.evaluate(&member.obj, self) {
                ConstValue::Array(values) => property
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| values.get(index).cloned())
                    .unwrap_or(ConstValue::Unknown),
                ConstValue::Object(values) => values
                    .get(&property)
                    .cloned()
                    .unwrap_or(ConstValue::Unknown),
                _ => ConstValue::Unknown,
            };
        }
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool {
        self.scopes.unshadowed_global_at(name, span)
    }
}

fn rooted_value(chain: &str) -> Value {
    let mut segments = chain.split('.');
    let root = segments.next().unwrap_or_default().to_string();
    Value::RootedMember {
        root,
        path: segments.map(str::to_string).collect(),
    }
}
