//! Declaration-only lexical scope planning.
//!
//! The planner establishes binding visibility and the structural identity of
//! every scope-forming node. It deliberately does not record assignments,
//! aliases, calls, or other source-order facts.

use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};
use swc_common::Spanned;
use swc_ecma_ast::{
    ArrowExpr, BlockStmt, CatchClause, ClassDecl, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function,
    Ident, ImportDecl, Lit, MemberExpr, Pat, PropName, SwitchStmt, VarDecl, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::{ScopeShape, ScopeShapeTable};
use crate::analysis::{
    name::NameTable,
    scope::{BindingProvenance, LexicalScope, ScopeId, ScopeKind},
    syntax::{collect_pat_bindings, module_export_name},
};

/// Immutable declaration result consumed by [`super::ScopeCollector`].
pub(in crate::analysis::scope) struct ScopePlan {
    pub(super) names: NameTable,
    pub(super) scopes: Vec<LexicalScope>,
    pub(super) scope_shapes: ScopeShapeTable,
    pub(super) name_exhausted: bool,
}

pub(in crate::analysis::scope) struct ScopePlanner {
    names: NameTable,
    scopes: Vec<LexicalScope>,
    stack: Vec<usize>,
    scope_shapes: ScopeShapeTable,
    name_exhausted: bool,
}

impl ScopePlanner {
    pub(in crate::analysis::scope) fn new(
        program_span: swc_common::Span,
        names: NameTable,
    ) -> Self {
        let mut names = names;
        let mut name_exhausted = false;
        for name in [
            "this",
            "eval",
            "Function",
            "prototype",
            "call",
            "apply",
            "bind",
        ] {
            if names.intern(name).is_err() {
                name_exhausted = true;
            }
        }
        Self {
            names,
            scopes: vec![LexicalScope {
                span: program_span,
                depth: 0,
                kind: ScopeKind::Program,
                parent: None,
                bindings: BTreeMap::new(),
            }],
            stack: vec![0],
            scope_shapes: ScopeShapeTable::new(),
            name_exhausted,
        }
    }

    pub(in crate::analysis::scope) fn finish(self) -> ScopePlan {
        ScopePlan {
            names: self.names,
            scopes: self.scopes,
            scope_shapes: self.scope_shapes,
            name_exhausted: self.name_exhausted,
        }
    }

    fn current_scope(&self) -> ScopeId {
        ScopeId::from(self.stack.last().copied().unwrap_or(0))
    }

    fn insert(&mut self, scope: ScopeId, name: impl Into<SmolStr>, provenance: BindingProvenance) {
        let name = name.into();
        let Ok(name_id) = self.names.intern(name.as_str()) else {
            self.name_exhausted = true;
            return;
        };
        self.scopes[scope.index()]
            .bindings
            .insert(name_id, provenance);
    }

    fn insert_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    fn insert_import(&mut self, scope: ScopeId, import: &ImportDecl) {
        let module = import.src.value.to_string_lossy().to_smolstr();
        for specifier in &import.specifiers {
            match specifier {
                swc_ecma_ast::ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_smolstr();
                    let export = named
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), module_export_name);
                    self.insert(
                        scope,
                        local,
                        BindingProvenance::ModuleExport {
                            module: module.clone(),
                            export,
                        },
                    );
                }
                swc_ecma_ast::ImportSpecifier::Namespace(namespace) => self.insert(
                    scope,
                    namespace.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
                swc_ecma_ast::ImportSpecifier::Default(default) => self.insert(
                    scope,
                    default.local.sym.to_smolstr(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
            }
        }
    }

    fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        let mut bindings = std::collections::BTreeSet::new();
        collect_pat_bindings(pat, &mut bindings);
        for binding in bindings {
            self.insert_local(scope, binding);
        }
    }

    fn binding_scope(&self, kind: swc_ecma_ast::VarDeclKind) -> ScopeId {
        if kind != swc_ecma_ast::VarDeclKind::Var {
            return self.current_scope();
        }
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|index| {
                matches!(
                    self.scopes[*index].kind,
                    ScopeKind::Program | ScopeKind::Function
                )
            })
            .map_or_else(|| ScopeId::from(0), ScopeId::from)
    }

    pub(super) fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) {
        let parent = self.current_scope();
        let index = self.scopes.len();
        self.scopes.push(LexicalScope {
            span,
            depth: self.stack.len(),
            kind,
            parent: Some(parent),
            bindings: BTreeMap::new(),
        });
        self.scope_shapes.record(ScopeShape {
            scope_id: ScopeId::from(index),
            kind,
            span,
            parent: Some(parent),
        });
        self.stack.push(index);
    }

    pub(super) fn pop_scope(&mut self) {
        let _ = self.stack.pop();
    }
}

impl Visit for ScopePlanner {
    fn visit_ident(&mut self, ident: &Ident) {
        if self.names.intern(ident.sym.as_ref()).is_err() {
            self.name_exhausted = true;
        }
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if let Some(property) = crate::analysis::syntax::member_property_name(&member.prop)
            && self.names.intern(property.as_str()).is_err()
        {
            self.name_exhausted = true;
        }
        if let Some(path) = crate::analysis::syntax::member_expression_chain(member) {
            for segment in path.segments() {
                if self.names.intern(segment).is_err() {
                    self.name_exhausted = true;
                }
            }
        }
        member.visit_children_with(self);
    }

    fn visit_prop_name(&mut self, property: &PropName) {
        if let Some(property) = crate::analysis::syntax::property_name(property)
            && self.names.intern(property.as_str()).is_err()
        {
            self.name_exhausted = true;
        }
        property.visit_children_with(self);
    }

    fn visit_lit(&mut self, literal: &Lit) {
        if let Lit::Str(value) = literal
            && self
                .names
                .intern(value.value.to_string_lossy().as_ref())
                .is_err()
        {
            self.name_exhausted = true;
        }
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.insert_import(self.current_scope(), import);
    }

    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        let scope = self.binding_scope(declaration.kind);
        for declarator in &declaration.decls {
            self.insert_pat_locals(scope, &declarator.name);
            declarator.name.visit_with(self);
            declarator.init.visit_with(self);
        }
    }

    fn visit_fn_decl(&mut self, declaration: &FnDecl) {
        let parent = self.current_scope();
        self.insert_local(parent, declaration.ident.sym.to_string());
        self.push_scope(declaration.function.span, ScopeKind::Function);
        let scope = self.current_scope();
        for parameter in &declaration.function.params {
            self.insert_pat_locals(scope, &parameter.pat);
            parameter.pat.visit_with(self);
        }
        declaration.function.decorators.visit_with(self);
        declaration.function.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        self.insert_local(self.current_scope(), declaration.ident.sym.to_string());
        declaration.class.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        self.push_scope(function.span, ScopeKind::Function);
        let scope = self.current_scope();
        for parameter in &function.params {
            self.insert_pat_locals(scope, &parameter.pat);
            parameter.pat.visit_with(self);
        }
        function.decorators.visit_with(self);
        function.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.push_scope(arrow.span, ScopeKind::Function);
        let scope = self.current_scope();
        for parameter in &arrow.params {
            self.insert_pat_locals(scope, parameter);
            parameter.visit_with(self);
        }
        arrow.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.push_scope(block.span, ScopeKind::Block);
        block.stmts.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_stmt(&mut self, statement: &ForStmt) {
        self.push_scope(statement.span, ScopeKind::Block);
        statement.init.visit_with(self);
        statement.test.visit_with(self);
        statement.update.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, statement: &ForInStmt) {
        self.push_scope(statement.span, ScopeKind::Block);
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_of_stmt(&mut self, statement: &ForOfStmt) {
        self.push_scope(statement.span, ScopeKind::Block);
        statement.left.visit_with(self);
        statement.right.visit_with(self);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_switch_stmt(&mut self, statement: &SwitchStmt) {
        statement.discriminant.visit_with(self);
        self.push_scope(statement.span, ScopeKind::Block);
        statement.cases.visit_with(self);
        self.pop_scope();
    }

    fn visit_with_stmt(&mut self, statement: &WithStmt) {
        statement.obj.visit_with(self);
        self.push_scope(statement.body.span(), ScopeKind::Dynamic);
        statement.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause) {
        self.push_scope(clause.span, ScopeKind::Block);
        let scope = self.current_scope();
        if let Some(parameter) = &clause.param {
            self.insert_pat_locals(scope, parameter);
            parameter.visit_with(self);
        }
        clause.body.stmts.visit_with(self);
        self.pop_scope();
    }
}
