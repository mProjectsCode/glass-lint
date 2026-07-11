//! Position-sensitive expression resolution.
//!
//! `ScopeGraph` supplies lexical declarations and historical assignments.
//! `Resolver` is the single adapter from those low-level facts to the values
//! consumed by matchers, so callers never make matching decisions from raw
//! identifier spelling.

use std::cell::RefCell;

use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr, Program};

use super::{
    ast::{SymbolCallProvenance, SymbolMemberProvenance},
    events::EventLog,
    scope::ScopeGraph,
    value::{Value, ValueArena, ValueId},
};

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
}

#[derive(Debug)]
pub(super) struct Resolver {
    scopes: ScopeGraph,
    events: EventLog,
    values: RefCell<ValueArena>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            scopes: ScopeGraph::default(),
            events: EventLog::default(),
            values: RefCell::new(ValueArena::default()),
        }
    }
}

impl Resolver {
    pub(super) fn collect(program: &Program) -> Self {
        let scopes = ScopeGraph::collect(program);
        Self {
            events: EventLog::collect(program).with_scopes(|span| scopes.scope_at(span)),
            scopes,
            values: RefCell::new(ValueArena::default()),
        }
    }

    pub(super) fn events_are_source_ordered(&self) -> bool {
        self.events.is_source_ordered()
    }

    pub(super) fn value_is_known(&self, id: ValueId) -> bool {
        id != ValueId::UNKNOWN && self.values.borrow().get(id).is_some()
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
        let scoped_call = self.scopes.call_provenance(ident.sym.as_ref(), ident.span);
        let rooted_chain = self.scopes.callable_member_chain(ident);
        let id = self.intern_call_value(&scoped_call, rooted_chain.as_deref());
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
        ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member,
        }
    }

    pub(super) fn bound_string_arguments(&self, ident: &Ident) -> Option<Vec<Option<String>>> {
        self.scopes.bound_string_arguments(ident)
    }

    pub(super) fn scope_chain_at(&self, span: swc_common::Span) -> Vec<usize> {
        self.scopes.scope_chain_at(span)
    }

    pub(super) fn has_assignment_at(&self, name: &str, span: swc_common::Span) -> bool {
        self.scopes.has_assignment_at(name, span)
    }

    pub(super) fn resolve_member(&self, member: &MemberExpr) -> ResolvedValue {
        let syntactic = self.scopes.member_chain(member);
        // Prefer the alias-expanded path. Falling back to a rooted member keeps
        // direct global/`this` access available when no local alias is present.
        let rooted_chain = syntactic
            .as_deref()
            .and_then(|chain| self.scopes.resolve_member_chain(member, chain))
            .or_else(|| self.scopes.rooted_member_chain(member));
        let module_member = syntactic
            .as_deref()
            .and_then(|chain| self.scopes.member_call_provenance_for_chain(member, chain));
        let scoped_call = match &module_member {
            Some(SymbolMemberProvenance::ModuleNamespace { module, member }) => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: member.clone(),
                }
            }
            None => SymbolCallProvenance::Local,
        };
        let returned_member = self.scopes.returned_member(member);
        let id = self.intern_call_value(&scoped_call, rooted_chain.as_deref());
        let call = self.call_provenance_for_value(id);
        if let Some(SymbolMemberProvenance::ModuleNamespace { module, .. }) = &module_member {
            self.values
                .borrow_mut()
                .intern(Value::ModuleNamespace(module.clone()));
        }
        ResolvedValue {
            id,
            rooted_chain,
            call,
            module_member,
            returned_member,
        }
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
            Expr::Call(call) => self.resolve_call_expression(call),
            Expr::New(_) => self.fresh_object_value(),
            _ => self.unknown(),
        }
    }

    pub(super) fn expr_call_provenance(&self, expr: &Expr) -> Option<SymbolCallProvenance> {
        let value = self.resolve_expr(expr);
        (!matches!(value.call, SymbolCallProvenance::Local)).then_some(value.call)
    }

    pub(super) fn static_string_expr(&self, expr: &Expr) -> Option<String> {
        let value = self.scopes.static_string_expr(expr)?;
        self.values
            .borrow_mut()
            .intern(Value::StaticString(value.clone()));
        Some(value)
    }

    pub(super) fn static_string_array_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        self.scopes.static_string_array_expr(expr)
    }

    pub(super) fn object_keys_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        let keys = self.scopes.object_keys_expr(expr)?;
        let mut values = self.values.borrow_mut();
        let unknown = ValueId::UNKNOWN;
        values.intern(Value::StaticObject(
            keys.iter().cloned().map(|key| (key, unknown)).collect(),
        ));
        Some(keys)
    }

    pub(super) fn rooted_expr_chain(&self, expr: &Expr) -> Option<String> {
        self.scopes.rooted_expr_chain(expr)
    }

    pub(super) fn member_chain(&self, member: &MemberExpr) -> Option<String> {
        self.scopes.member_chain(member)
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
        }
    }

    fn resolve_call_expression(&self, call: &swc_ecma_ast::CallExpr) -> ResolvedValue {
        // Calls normally produce fresh, opaque values. `.bind` is the modeled
        // exception because it preserves a callable's target and arguments.
        let Callee::Expr(callee) = &call.callee else {
            return self.unknown();
        };
        let Expr::Member(member) = &**callee else {
            return self.fresh_object_value();
        };
        if super::ast::member_prop_name(&member.prop).as_deref() != Some("bind") {
            return self.fresh_object_value();
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

    fn intern_call_value(&self, call: &SymbolCallProvenance, rooted: Option<&str>) -> ValueId {
        let value = match call {
            SymbolCallProvenance::Global { name } => Value::Global(name.clone()),
            SymbolCallProvenance::ModuleExport { module, export } => Value::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            SymbolCallProvenance::Local => rooted.map_or(Value::Local, rooted_value),
        };
        let id = self.values.borrow_mut().intern(value);
        debug_assert!(self.values.borrow().get(id).is_some());
        id
    }

    /// Convert the canonical value back into matcher provenance. This keeps
    /// the arena authoritative for call identity: scope collection supplies a
    /// candidate once, but matchers never consume a separately reconstructed
    /// spelling. Unknown or exhausted values are local and therefore fail
    /// closed for strict global/module matchers.
    fn call_provenance_for_value(&self, id: ValueId) -> SymbolCallProvenance {
        let Some(value) = self.values.borrow().get(id).cloned() else {
            return SymbolCallProvenance::Local;
        };
        match value {
            Value::Global(name) => SymbolCallProvenance::Global { name },
            Value::ModuleExport { module, export } => {
                SymbolCallProvenance::ModuleExport { module, export }
            }
            Value::Callable(callable) => self.call_provenance_for_value(callable.target),
            _ => SymbolCallProvenance::Local,
        }
    }

    fn fresh_object_value(&self) -> ResolvedValue {
        let Some(object) = self.values.borrow_mut().allocate_object_id() else {
            return self.unknown();
        };
        self.static_value(Value::Object(object))
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
