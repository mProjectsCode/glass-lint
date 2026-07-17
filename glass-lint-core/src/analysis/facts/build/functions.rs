//! Function boundaries and parameter-path facts for local and project flow.
//!
//! Enter/exit facts identify the lexical owner and parameter paths of each
//! callable body. This lets local and project flow transfer values through
//! supported wrappers without treating nested functions as one scope.

use swc_common::Spanned;
use swc_ecma_ast::ClassMethod;

use super::{
    ArrowExpr, BinExpr, BinaryOp, ClassDecl, ClassExpr, FactBuilder, FactKind, FactPayload, FnDecl,
    Function, FunctionBoundary, Pat, PathId, Span, VisitWith,
};

impl FactBuilder<'_> {
    /// Return the proven class provenance for the current non-static method.
    pub(super) fn current_class(&self) -> Option<(String, String)> {
        self.traversal.current_class()
    }

    /// Emit a function boundary with parameter bindings owned by its body.
    pub(super) fn emit_function_fact(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = (usize, Pat)>,
        boundary: FunctionBoundary,
    ) {
        // The owner is the enclosing function, not the function being entered;
        // this distinction keeps nested effects attached to the right scope.
        let scope = self.scope_at(span);
        let id = self.resolver.function_id_for_scope(scope);
        let owner = self
            .resolver
            .scope_parent(scope)
            .map_or(id, |scope| self.resolver.function_id_for_scope(scope));
        let mut parameter_bindings = Vec::new();
        for (parameter_index, parameter) in parameters {
            self.parameter_bindings(
                &parameter,
                parameter_index,
                PathId::EMPTY,
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
                parameters: parameter_bindings,
                boundary,
            },
        );
    }

    pub(super) fn record_function_decl(&mut self, function: &FnDecl) {
        self.record_local(function.ident.sym.to_string());
        self.traversal.enter_function();
        function.visit_children_with(self);
        self.traversal.leave_function();
    }

    pub(super) fn record_function(&mut self, function: &Function) {
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Enter,
        );
        self.traversal.enter_function();
        function.visit_children_with(self);
        self.traversal.leave_function();
        self.emit_function_fact(
            function.span(),
            function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone())),
            FunctionBoundary::Exit,
        );
    }

    pub(super) fn record_arrow(&mut self, arrow: &ArrowExpr) {
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Enter,
        );
        arrow.body.visit_with(self);
        self.emit_function_fact(
            arrow.span(),
            arrow.params.iter().cloned().enumerate(),
            FunctionBoundary::Exit,
        );
    }

    pub(super) fn record_class_method(&mut self, method: &ClassMethod) {
        let parameters = || {
            method
                .function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| (index, parameter.pat.clone()))
        };
        self.emit_function_fact(
            method.function.span(),
            parameters(),
            FunctionBoundary::Enter,
        );
        if method.is_static {
            self.traversal.enter_static_method();
        }
        if let Some(body) = method.function.body.as_ref() {
            body.visit_with(self);
        }
        self.emit_function_fact(method.function.span(), parameters(), FunctionBoundary::Exit);
        if method.is_static {
            self.traversal.leave_static_method();
        }
    }

    pub(super) fn record_class_decl(&mut self, class_decl: &ClassDecl) {
        self.record_local(class_decl.ident.sym.to_string());
        let name = class_decl.ident.sym.to_string();
        let provenance = class_decl
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        self.emit(
            FactKind::Declaration,
            class_decl.ident.span(),
            FactPayload::Class {
                name,
                provenance: provenance.clone(),
            },
        );
        self.traversal.enter_class(provenance);
        class_decl.visit_children_with(self);
        self.traversal.leave_class();
    }

    pub(super) fn record_class_expr(&mut self, class_expr: &ClassExpr) {
        let provenance = class_expr
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr));
        if let Some(ident) = &class_expr.ident {
            self.emit(
                FactKind::Declaration,
                ident.span(),
                FactPayload::Class {
                    name: ident.sym.to_string(),
                    provenance: provenance.clone(),
                },
            );
        }
        self.traversal.enter_class(provenance);
        class_expr.visit_children_with(self);
        self.traversal.leave_class();
    }

    pub(super) fn record_instanceof(&mut self, binary: &BinExpr) {
        if binary.op == BinaryOp::InstanceOf {
            let provenance = self.resolver.class_provenance(&binary.right);
            self.emit(
                FactKind::Reference,
                binary.right.span(),
                FactPayload::Class {
                    name: String::new(),
                    provenance,
                },
            );
        }
        binary.visit_children_with(self);
    }
}
