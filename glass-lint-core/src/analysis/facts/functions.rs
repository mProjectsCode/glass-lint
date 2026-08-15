//! Function boundaries and parameter-path facts for local and project flow.
//!
//! Enter/exit facts identify the lexical owner and parameter paths of each
//! callable body. This lets local and project flow transfer values through
//! supported wrappers without treating nested functions as one scope.

use smol_str::{SmolStr, ToSmolStr};
use swc_common::Spanned;
use swc_ecma_ast::ClassMethod;

use crate::analysis::{
    facts::{
        ArrowExpr, BinExpr, BinaryOp, ClassDecl, ClassExpr, ClassFactRole, Expr, FactBuilder,
        FactPayload, FnDecl, Function, FunctionBoundary, Pat, PathId, Span, VisitWith,
    },
    model::fact::ClassIdentity,
    syntax::literal_member_property_name,
};

#[derive(Clone, Copy)]
enum FunctionBodyKind {
    Function,
    Arrow,
    InstanceMethod,
    StaticMethod,
}

impl FunctionBodyKind {
    fn tracks_function_depth(self) -> bool {
        matches!(self, Self::Function)
    }

    fn is_static_method(self) -> bool {
        matches!(self, Self::StaticMethod)
    }
}

impl FactBuilder<'_, '_> {
    fn with_function_context(&mut self, kind: FunctionBodyKind, visit: impl FnOnce(&mut Self)) {
        let enclosing = self.traversal.current_function();
        if kind.tracks_function_depth() {
            self.traversal.push_function();
        }
        if kind.is_static_method() {
            self.traversal.push_static_method();
        }
        visit(self);
        if kind.is_static_method() {
            self.traversal.pop_static_method();
        }
        if kind.tracks_function_depth() {
            self.traversal.pop_function();
        }
        self.traversal.set_function(enclosing);
    }

    fn with_class_context(
        &mut self,
        provenance: Option<ClassIdentity>,
        visit: impl FnOnce(&mut Self),
    ) {
        self.traversal.push_class(provenance);
        visit(self);
        self.traversal.pop_class();
    }

    /// Return the proven class provenance for the current non-static method.
    pub(super) fn current_class(&self) -> Option<ClassIdentity> {
        self.traversal.current_class()
    }

    /// Emit a function boundary with parameter bindings owned by its body.
    ///
    /// Only `Enter` facts carry resolved parameter bindings; `Exit` facts are
    /// flow markers that the projector uses to restore the calling frame and
    /// never read parameter data.
    pub(super) fn emit_function_fact<'pat>(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = (usize, &'pat Pat)>,
    ) {
        let Some(scope) = self.scope_at(span) else {
            return;
        };
        let id = self.resolver.function_scope_at(scope);
        self.traversal.set_function(id);
        let mut bindings = Vec::new();
        for (parameter_index, parameter) in parameters {
            self.parameter_bindings(
                parameter,
                parameter_index,
                PathId::EMPTY,
                None,
                false,
                &mut bindings,
            );
        }
        self.stream.register_function_parameters(id, bindings);
        self.emit(
            span,
            FactPayload::Function {
                id,
                boundary: FunctionBoundary::Enter,
            },
        );
    }

    fn emit_function_exit_fact(&mut self, span: Span) {
        let Some(scope) = self.scope_at(span) else {
            return;
        };
        let id = self.resolver.function_scope_at(scope);
        self.traversal.set_function(id);
        self.emit(
            span,
            FactPayload::Function {
                id,
                boundary: FunctionBoundary::Exit,
            },
        );
    }

    pub(super) fn record_function_decl(&mut self, function: &FnDecl) {
        self.record_local(function.ident.sym.to_string());
        self.with_function_context(FunctionBodyKind::Function, |builder| {
            function.visit_children_with(builder);
        });
    }

    pub(super) fn record_function(&mut self, function: &Function) {
        let parameters = function
            .params
            .iter()
            .enumerate()
            .map(|(index, parameter)| (index, &parameter.pat));
        self.record_function_body(
            function.span(),
            parameters,
            FunctionBodyKind::Function,
            |builder| {
                function.visit_children_with(builder);
            },
        );
    }

    fn record_function_body<'pat>(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = (usize, &'pat Pat)>,
        kind: FunctionBodyKind,
        visit_body: impl FnOnce(&mut Self),
    ) {
        self.emit_function_fact(span, parameters);
        self.with_function_context(kind, |builder| {
            visit_body(builder);
            builder.emit_function_exit_fact(span);
        });
    }

    pub(super) fn record_arrow(&mut self, arrow: &ArrowExpr) {
        let parameters = arrow.params.iter().enumerate();
        self.record_function_body(
            arrow.span(),
            parameters,
            FunctionBodyKind::Arrow,
            |builder| {
                arrow.body.visit_with(builder);
            },
        );
    }

    pub(super) fn record_class_method(&mut self, method: &ClassMethod) {
        let parameters = method
            .function
            .params
            .iter()
            .enumerate()
            .map(|(index, parameter)| (index, &parameter.pat));
        self.record_function_body(
            method.function.span(),
            parameters,
            if method.is_static {
                FunctionBodyKind::StaticMethod
            } else {
                FunctionBodyKind::InstanceMethod
            },
            |builder| {
                if let Some(body) = method.function.body.as_ref() {
                    body.visit_with(builder);
                }
            },
        );
    }

    pub(super) fn record_class_decl(&mut self, class_decl: &ClassDecl) {
        self.record_local(class_decl.ident.sym.to_smolstr());
        let name = class_decl.ident.sym.to_smolstr();
        let provenance = class_decl
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr))
            .map(ClassIdentity::from);
        if let Some(provenance) = provenance.clone() {
            let value = self.resolver.resolve_ident_id(&class_decl.ident);
            self.provenance
                .record_class_origin(value, provenance, self.resolver.budget);
        }
        self.emit(
            class_decl.ident.span(),
            FactPayload::Class {
                name: Some(name),
                role: ClassFactRole::Declaration,
                provenance: provenance.clone(),
            },
        );
        self.record_class_operand(class_decl.class.super_class.as_deref());
        self.with_class_context(provenance, |builder| {
            class_decl.visit_children_with(builder);
        });
    }

    pub(super) fn record_class_expr(&mut self, class_expr: &ClassExpr) {
        let provenance = class_expr
            .class
            .super_class
            .as_deref()
            .and_then(|expr| self.resolver.class_provenance(expr))
            .map(ClassIdentity::from);
        if let Some(ident) = &class_expr.ident {
            self.emit(
                ident.span(),
                FactPayload::Class {
                    name: Some(ident.sym.to_smolstr()),
                    role: ClassFactRole::Declaration,
                    provenance: provenance.clone(),
                },
            );
        }
        self.record_class_operand(class_expr.class.super_class.as_deref());
        self.with_class_context(provenance, |builder| {
            class_expr.visit_children_with(builder);
        });
    }

    pub(super) fn record_instanceof(&mut self, binary: &BinExpr) {
        if binary.op == BinaryOp::InstanceOf {
            let provenance = self
                .resolver
                .class_provenance(&binary.right)
                .map(ClassIdentity::from);
            self.emit(
                binary.right.span(),
                FactPayload::Class {
                    name: Self::class_operand_name(&binary.right),
                    role: ClassFactRole::InstanceofOperand,
                    provenance,
                },
            );
        }
        binary.visit_children_with(self);
    }

    fn record_class_operand(&mut self, expr: Option<&Expr>) {
        let Some(expr) = expr else {
            return;
        };
        let provenance = self
            .resolver
            .class_provenance(expr)
            .map(ClassIdentity::from);
        self.emit(
            expr.span(),
            FactPayload::Class {
                name: Self::class_operand_name(expr),
                role: ClassFactRole::SuperclassOperand,
                provenance,
            },
        );
    }

    fn class_operand_name(expr: &Expr) -> Option<SmolStr> {
        match expr {
            Expr::Ident(ident) => Some(ident.sym.to_smolstr()),
            Expr::Member(member) => literal_member_property_name(&member.prop),
            Expr::Paren(paren) => Self::class_operand_name(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| Self::class_operand_name(expr)),
            Expr::TsAs(value) => Self::class_operand_name(&value.expr),
            Expr::TsNonNull(value) => Self::class_operand_name(&value.expr),
            Expr::TsSatisfies(value) => Self::class_operand_name(&value.expr),
            Expr::TsTypeAssertion(value) => Self::class_operand_name(&value.expr),
            _ => None,
        }
    }
}
