use super::{
    CallExpr, Callee, Expr, Lit, ResolvedValue, Resolver, SymbolCallProvenance, Value, ValueId,
};

impl Resolver {
    pub(super) fn call_provenance_at(
        &self,
        id: ValueId,
        rooted: Option<&str>,
        span: swc_common::Span,
    ) -> SymbolCallProvenance {
        let provenance = self.call_provenance_for_value(id);
        if provenance != SymbolCallProvenance::Local {
            return provenance;
        }
        rooted
            .and_then(|chain| self.scopes.global_callable_member_at(chain, span))
            .map_or(SymbolCallProvenance::Local, |name| {
                SymbolCallProvenance::Global { name }
            })
    }

    pub(in crate::analysis) fn require_module_name(&self, call: &CallExpr) -> Option<String> {
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

    pub(in crate::analysis) fn resolve_call_expression(
        &self,
        call: &swc_ecma_ast::CallExpr,
    ) -> ResolvedValue {
        // Calls normally produce fresh, opaque values. `.bind` is the modeled
        // exception because it preserves a callable's target and arguments.
        let Callee::Expr(callee) = &call.callee else {
            return self.unknown();
        };
        let Expr::Member(member) = &**callee else {
            return self.fresh_object_value_at(call.span);
        };
        if crate::analysis::syntax::member_prop_name(&member.prop).as_deref() != Some("bind") {
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
        self.static_value(Value::Callable(crate::analysis::value::CallableValue {
            target,
            receiver,
            bound_arguments,
        }))
    }

    pub(in crate::analysis) fn intern_call_value(
        &self,
        call: &SymbolCallProvenance,
        rooted: Option<&str>,
        binding: Option<crate::analysis::value::BindingKey>,
    ) -> ValueId {
        let value = match call {
            SymbolCallProvenance::Global { name } => Value::Global(name.clone()),
            SymbolCallProvenance::ModuleExport { module, export } => Value::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            SymbolCallProvenance::Local => rooted.map_or(Value::Local, Resolver::rooted_value),
        };
        let id = self.values.borrow_mut().intern_with_binding(value, binding);
        debug_assert!(self.values.borrow().get(id).is_some());
        id
    }

    /// Convert the canonical value back into matcher provenance. This keeps
    /// the arena authoritative for call identity: scope collection supplies a
    /// typed seed once, but matchers never consume a separately reconstructed
    /// spelling. Unknown or exhausted values are local and therefore fail
    /// closed for strict global/module matchers.
    pub(in crate::analysis) fn call_provenance_for_value(
        &self,
        id: ValueId,
    ) -> SymbolCallProvenance {
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
            Value::RootedMember { root, path }
                if path.is_empty() && self.scopes.is_configured_global(&root) =>
            {
                SymbolCallProvenance::Global { name: root }
            }
            _ => SymbolCallProvenance::Local,
        }
    }
}
