//! Call-site provenance and callable-value resolution.
//!
//! Ordinary calls produce opaque fresh values. Only literal dynamic imports
//! and modeled `.bind()` calls preserve callable identity, and CommonJS
//! recognition requires an unshadowed global `require` binding.

use smol_str::ToSmolStr;

use crate::analysis::{
    resolution::{
        CallExpr, Callee, Expr, Lit, ResolvedValue, Resolver, SymbolCallProvenance, Value, ValueId,
    },
    syntax::{BudgetComponent, UnknownReason},
    value::MAX_VALUES,
};

impl Resolver<'_> {
    /// Recover global callable provenance for a resolved value at a position.
    pub(super) fn call_provenance_at(
        &self,
        id: ValueId,
        rooted: Option<&glass_lint_datastructures::SymbolPath>,
        span: swc_common::Span,
    ) -> SymbolCallProvenance {
        let provenance = self.call_provenance_for_value(id);
        if matches!(
            provenance,
            SymbolCallProvenance::Global { .. } | SymbolCallProvenance::ModuleExport { .. }
        ) {
            return provenance;
        }
        rooted
            .and_then(|chain| self.scopes.global_callable_member_at(chain, span))
            .map_or(provenance, |name| SymbolCallProvenance::Global {
                name: name.to_smolstr(),
            })
    }

    /// Return a literal module name for an unshadowed global `require` call.
    pub(in crate::analysis) fn require_module_name(&mut self, call: &CallExpr) -> Option<String> {
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

    /// Check that an identifier is the unshadowed CommonJS/global loader name.
    pub(in crate::analysis) fn is_unshadowed_commonjs_name(
        &self,
        ident: &swc_ecma_ast::Ident,
        name: &str,
    ) -> bool {
        ident.sym == name && self.scopes.unshadowed_unbound_at(name, ident.span)
    }

    /// Resolve a call result, preserving only supported callable wrappers.
    pub(in crate::analysis) fn resolve_call_expression(
        &mut self,
        call: &swc_ecma_ast::CallExpr,
    ) -> std::sync::Arc<ResolvedValue> {
        if matches!(call.callee, Callee::Import(_))
            && let Some(Expr::Lit(Lit::Str(specifier))) = call.args.first().map(|arg| &*arg.expr)
        {
            let module = specifier.value.to_string_lossy().to_string();
            let id = self.intern_call_value(
                &SymbolCallProvenance::ModuleExport {
                    module: module.into(),
                    export: "*".into(),
                },
                None,
                None,
            );
            return std::sync::Arc::new(ResolvedValue {
                id,
                rooted_chain: None,
                call: self.call_provenance_for_value(id),
                module_member: None,
                returned_member: None,
                bound_arguments: None,
                syntactic_chain: None,
            });
        }
        let Callee::Expr(callee) = &call.callee else {
            return Self::unknown();
        };
        let Expr::Member(member) = &**callee else {
            return self.fresh_object_value_at(call.span);
        };
        if crate::analysis::syntax::member_property_name(&member.prop).as_deref() != Some("bind") {
            return self.fresh_object_value_at(call.span);
        }
        let target = self.resolve_expr_id(&member.obj);
        let receiver = call
            .args
            .first()
            .map(|argument| self.resolve_expr_id(&argument.expr));
        let bound_arguments = call
            .args
            .iter()
            .skip(1)
            .map(|argument| self.resolve_expr_id(&argument.expr))
            .collect();
        self.static_value(Value::Callable(crate::analysis::value::CallableValue::new(
            target,
            receiver,
            bound_arguments,
        )))
    }

    /// Intern callable/module/global value identity with optional binding
    /// scope.
    pub(in crate::analysis) fn intern_call_value(
        &mut self,
        call: &SymbolCallProvenance,
        rooted: Option<&glass_lint_datastructures::SymbolPath>,
        binding: Option<crate::analysis::value::BindingKey>,
    ) -> ValueId {
        let value = match call {
            SymbolCallProvenance::Global { name } => Value::Global(name.clone()),
            SymbolCallProvenance::ModuleExport { module, export } => Value::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            SymbolCallProvenance::Local => {
                rooted.map_or(Value::Local, |path| self.rooted_value(path))
            }
            SymbolCallProvenance::Unknown(_) => Value::Unknown,
        };
        let id = self.values.intern_with_binding(value, binding);
        debug_assert!(self.values.get(id).is_some());
        id
    }

    /// Convert the canonical value back into matcher provenance. This keeps
    /// the arena authoritative for call identity: scope collection supplies a
    /// typed seed once, but matchers never consume a separately reconstructed
    /// spelling. Unknown or exhausted values remain non-matchable and fail
    /// closed for strict global/module matchers.
    pub(in crate::analysis) fn call_provenance_for_value(
        &self,
        id: ValueId,
    ) -> SymbolCallProvenance {
        if id == ValueId::UNKNOWN {
            return if self.value_arena_exhausted() {
                SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                    component: BudgetComponent::Values,
                    limit: MAX_VALUES,
                    observed: None,
                })
            } else {
                SymbolCallProvenance::Unknown(UnknownReason::Unsupported)
            };
        }
        let mut current = id;
        loop {
            let values = &self.values;
            let Some(value) = values.get(current) else {
                return SymbolCallProvenance::Unknown(UnknownReason::Missing);
            };
            match value {
                Value::Binding { target, .. } => current = *target,
                Value::Callable(callable) => current = callable.target(),
                Value::Global(name) => {
                    return SymbolCallProvenance::Global { name: name.clone() };
                }
                Value::ModuleExport { module, export } => {
                    return SymbolCallProvenance::ModuleExport {
                        module: module.clone(),
                        export: export.clone(),
                    };
                }
                Value::RootedMember { path } if path.is_root() => {
                    let Some(root) = path.first_segment().copied() else {
                        return SymbolCallProvenance::Unknown(UnknownReason::Unresolved);
                    };
                    let Some(name) = self.scopes.resolve_name_id(root) else {
                        return SymbolCallProvenance::Unknown(UnknownReason::Missing);
                    };
                    if self.scopes.is_global(name.as_str()) {
                        return SymbolCallProvenance::Global { name };
                    }
                    return SymbolCallProvenance::Unknown(UnknownReason::Unresolved);
                }
                Value::Unknown => {
                    return SymbolCallProvenance::Unknown(UnknownReason::Unsupported);
                }
                _ => return SymbolCallProvenance::Unknown(UnknownReason::Unresolved),
            }
        }
    }
}
