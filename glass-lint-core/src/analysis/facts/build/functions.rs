use super::*;

impl<'a> FactBuilder<'a> {
    pub(super) fn current_class(&self) -> Option<(String, String)> {
        self.class_stack.last().and_then(Clone::clone)
    }

    pub(super) fn emit_function_fact(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = (usize, Pat)>,
        boundary: FunctionBoundary,
    ) {
        let scope = self.scope_at(span);
        let id = self.resolver.function_id_for_scope(scope);
        let owner = self
            .resolver
            .scope_chain_at(span)
            .get(1)
            .copied()
            .map_or(id, |scope| self.resolver.function_id_for_scope(scope));
        let mut parameter_bindings = Vec::new();
        for (parameter_index, parameter) in parameters {
            self.parameter_bindings(
                &parameter,
                parameter_index,
                &mut Vec::new(),
                None,
                false,
                &mut parameter_bindings,
            );
        }
        self.emit(
            FactKind::Function,
            span,
            FactPayload::Function {
                id,
                owner,
                name: None,
                parameters: parameter_bindings,
                boundary,
            },
        );
    }
}
