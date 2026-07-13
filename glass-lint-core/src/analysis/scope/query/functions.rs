use super::*;

impl ScopeGraph {
    pub(in crate::analysis) fn function_scope_at(&self, scope: usize) -> FunctionId {
        let mut current = Some(scope);
        while let Some(index) = current {
            if let Some(function) = self.function_ids.get(&index) {
                return *function;
            }
            current = self.scopes[index].parent;
        }
        FunctionId(0)
    }

    pub(in crate::analysis) fn function_id_for_scope(&self, scope: usize) -> FunctionId {
        self.function_scope_at(scope)
    }

    pub(in crate::analysis) fn function_id_for_expr(&self, expr: &Expr) -> Option<FunctionId> {
        let Expr::Ident(ident) = expr else {
            return None;
        };
        let (scope, provenance) = self.binding_with_scope_at(ident.sym.as_ref(), ident.span)?;
        let function = self
            .function_bindings
            .get(&(scope, ident.sym.to_string()))
            .copied()
            .or_else(|| {
                self.function_aliases
                    .get(&(scope, ident.sym.to_string()))
                    .copied()
            })
            .or_else(|| {
                let target = match provenance {
                    BindingProvenance::ValueAlias { target }
                    | BindingProvenance::BoundCallable { target, .. } => target
                        .without_bind_suffix()
                        .unwrap_or_else(|| target.clone()),
                    _ => return None,
                };
                target
                    .is_root()
                    .then(|| self.function_binding_at(target.to_string().as_str(), ident.span))
                    .flatten()
            })?;
        let function_end = self.function_ids.iter().find_map(|(scope, candidate)| {
            (*candidate == function).then_some(self.scopes[*scope].span.hi)
        })?;
        let reassigned = self
            .assignments
            .get(&scope)
            .and_then(|assignments| assignments.get(ident.sym.as_ref()))
            .is_some_and(|assignments| {
                assignments.iter().any(|assignment| {
                    assignment.span.lo > function_end && assignment.span.lo <= ident.span.lo
                })
            });
        (!reassigned).then_some(function)
    }

    pub(in crate::analysis) fn function_binding_at(
        &self,
        name: &str,
        span: Span,
    ) -> Option<FunctionId> {
        let mut scope = self.scope_at(span);
        loop {
            if let Some(function) = self.function_bindings.get(&(scope, name.to_string())) {
                return Some(*function);
            }
            scope = self.scopes.get(scope)?.parent?;
        }
    }
}
