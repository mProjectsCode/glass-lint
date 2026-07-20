//! Declaration-only scope construction.
//!
//! This pass runs before provenance collection so lexical and hoisted names
//! are known at every source position, including uses before declarations.

use smol_str::ToSmolStr;
use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, BlockStmt, CatchClause, ClassDecl, ClassExpr, FnDecl, ForInStmt, ForOfStmt, ForStmt,
    Function, ImportDecl, ImportSpecifier, Param, SwitchStmt, VarDecl, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use crate::analysis::{
    scope::{BindingProvenance, ScopeKind, collect::LexicalScopeCollector},
    syntax::module_export_name,
};

pub(super) struct PredeclareVisitor<'a, 'b> {
    /// Collector whose scope tree and hoisted bindings this pass populates.
    pub(super) collector: &'a mut LexicalScopeCollector<'b>,
}

impl PredeclareVisitor<'_, '_> {
    /// Insert import bindings before ordinary source-order traversal.
    fn insert_import(&mut self, import: &ImportDecl) {
        let scope = self.collector.current_scope();
        let module = import.src.value.to_string_lossy().to_smolstr();
        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_smolstr();
                    let export = named
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), module_export_name);
                    self.collector.insert(
                        scope,
                        local,
                        BindingProvenance::ModuleExport {
                            module: module.clone(),
                            export,
                        },
                    );
                }
                ImportSpecifier::Namespace(namespace) => self.collector.insert(
                    scope,
                    namespace.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
                ImportSpecifier::Default(default) => self.collector.insert(
                    scope,
                    default.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
            }
        }
    }

    /// Enter a function scope and predeclare all parameter bindings.
    fn push_function(&mut self, span: Span, parameters: &[Param]) {
        self.collector.push_scope(span, ScopeKind::Function);
        let scope = self.collector.current_scope();
        for parameter in parameters {
            self.collector.insert_pat_locals(scope, &parameter.pat);
        }
    }

    /// Leave a function/block scope created by this visitor.
    fn pop_scope(&mut self) {
        debug_assert!(self.collector.stack.len() > 1);
        self.collector.pop_scope();
    }
}

impl Visit for PredeclareVisitor<'_, '_> {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.insert_import(import);
    }

    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        let scope = self.collector.binding_scope(declaration.kind);
        for declarator in &declaration.decls {
            self.collector.insert_pat_locals(scope, &declarator.name);
            if let Some(init) = declarator.init.as_deref() {
                init.visit_with(self);
            }
        }
    }

    fn visit_fn_decl(&mut self, declaration: &FnDecl) {
        let parent = self.collector.current_scope();
        self.collector
            .insert_local(parent, declaration.ident.sym.to_string());
        self.push_function(declaration.function.span, &declaration.function.params);
        declaration.function.decorators.visit_with(self);
        if let Some(body) = &declaration.function.body {
            body.visit_with(self);
        }
        self.pop_scope();
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        let scope = self.collector.current_scope();
        self.collector
            .insert_local(scope, declaration.ident.sym.to_string());
        declaration.class.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, expression: &ClassExpr) {
        expression.class.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        self.push_function(function.span, &function.params);
        function.decorators.visit_with(self);
        if let Some(body) = &function.body {
            body.visit_with(self);
        }
        self.pop_scope();
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.collector.push_scope(arrow.span, ScopeKind::Function);
        let scope = self.collector.current_scope();
        for parameter in &arrow.params {
            self.collector.insert_pat_locals(scope, parameter);
        }
        arrow.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.collector.push_scope(block.span, ScopeKind::Block);
        block.stmts.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_stmt(&mut self, statement: &ForStmt) {
        self.collector.push_scope(statement.span, ScopeKind::Block);
        statement.init.visit_with(self);
        statement.test.visit_with(self);
        statement.update.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, statement: &ForInStmt) {
        self.collector.push_scope(statement.span, ScopeKind::Block);
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_of_stmt(&mut self, statement: &ForOfStmt) {
        self.collector.push_scope(statement.span, ScopeKind::Block);
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_switch_stmt(&mut self, statement: &SwitchStmt) {
        statement.discriminant.visit_with(self);
        self.collector.push_scope(statement.span, ScopeKind::Block);
        statement.cases.visit_with(self);
        self.pop_scope();
    }

    fn visit_with_stmt(&mut self, statement: &WithStmt) {
        statement.obj.visit_with(self);
        self.collector
            .push_scope(statement.body.span(), ScopeKind::Dynamic);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause) {
        self.collector.push_scope(clause.span, ScopeKind::Block);
        let scope = self.collector.current_scope();
        if let Some(parameter) = &clause.param {
            self.collector.insert_pat_locals(scope, parameter);
        }
        clause.body.stmts.visit_with(self);
        self.pop_scope();
    }
}
