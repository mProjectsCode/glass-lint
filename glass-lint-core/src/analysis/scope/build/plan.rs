//! Declaration-only lexical scope planning.
//!
//! The planner establishes binding visibility and the structural identity of
//! every scope-forming node. It deliberately does not record assignments,
//! aliases, calls, or other source-order facts.

use glass_lint_datastructures::NameTable;
use smol_str::SmolStr;
use swc_ecma_ast::{
    ArrowExpr, ClassDecl, FnDecl, Function, Ident, ImportDecl, MemberExpr, Pat, PropName, VarDecl,
};

use crate::analysis::{
    SemanticBudget,
    scope::{
        BindingProvenance, LexicalScope, LexicalScopes, ScopeId, ScopeKind,
        build::{
            ScopeShape, ScopeShapeTable,
            bindings::{for_each_import_binding, for_each_pat_binding, var_binding_scope},
            traversal::{ScopeEntry, ScopePass},
        },
    },
};

/// Immutable declaration result consumed by [`super::ScopeCollector`].
pub(in crate::analysis::scope) struct ScopePlan {
    pub(super) program: ScopeId,
    pub(super) names: NameTable,
    pub(super) scopes: LexicalScopes,
    pub(super) scope_shapes: ScopeShapeTable,
    pub(super) name_exhausted: bool,
}

pub(in crate::analysis::scope) struct ScopePlanner<'a> {
    program: ScopeId,
    names: NameTable,
    scopes: LexicalScopes,
    stack: Vec<ScopeId>,
    scope_shapes: ScopeShapeTable,
    name_exhausted: bool,
    budget: &'a SemanticBudget,
}

impl ScopePlanner<'_> {
    pub(in crate::analysis::scope) fn new(
        program_span: swc_common::Span,
        names: NameTable,
        budget: &SemanticBudget,
    ) -> ScopePlanner<'_> {
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
            budget.try_charge();
            if names.intern(name).is_err() {
                name_exhausted = true;
            }
        }
        let mut scopes = LexicalScopes::new();
        let program = scopes.push(LexicalScope::new(program_span, 0, ScopeKind::Program, None));
        ScopePlanner {
            program,
            names,
            scopes,
            stack: vec![program],
            scope_shapes: ScopeShapeTable::new(),
            name_exhausted,
            budget,
        }
    }

    pub(in crate::analysis::scope) fn finish(self) -> ScopePlan {
        ScopePlan {
            program: self.program,
            names: self.names,
            scopes: self.scopes,
            scope_shapes: self.scope_shapes,
            name_exhausted: self.name_exhausted,
        }
    }

    fn current_scope(&self) -> Option<ScopeId> {
        self.stack.last().copied()
    }

    fn insert(&mut self, scope: ScopeId, name: impl Into<SmolStr>, provenance: BindingProvenance) {
        let name = name.into();
        self.budget.try_charge();
        let Ok(name_id) = self.names.intern(name.as_str()) else {
            self.name_exhausted = true;
            return;
        };
        if let Some(scope_data) = self.scopes.get_mut(scope) {
            scope_data.insert_binding(name_id, provenance);
        }
    }

    fn insert_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    fn insert_import(&mut self, scope: ScopeId, import: &ImportDecl) {
        for_each_import_binding(import, |name, provenance| {
            self.insert(scope, name, provenance);
        });
    }

    fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| self.insert_local(scope, binding));
    }

    fn binding_scope(&self, kind: swc_ecma_ast::VarDeclKind) -> Option<ScopeId> {
        if kind != swc_ecma_ast::VarDeclKind::Var {
            return self.current_scope();
        }
        var_binding_scope(&self.stack, &self.scopes)
    }

    pub(super) fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) {
        let Some(parent) = self.current_scope() else {
            return;
        };
        let scope_id = self.scopes.push(LexicalScope::new(
            span,
            self.stack.len(),
            kind,
            Some(parent),
        ));
        self.scope_shapes
            .record(ScopeShape::new(scope_id, kind, span, Some(parent)));
        self.stack.push(scope_id);
    }

    pub(super) fn pop_scope(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
}

impl ScopePass for ScopePlanner<'_> {
    fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) -> ScopeEntry {
        self.push_scope(span, kind);
        self.current_scope()
            .map_or(ScopeEntry::Rejected, ScopeEntry::Entered)
    }

    fn pop_scope(&mut self, entry: ScopeEntry) {
        if matches!(entry, ScopeEntry::Entered(_)) {
            self.pop_scope();
        }
    }

    fn current_scope(&self) -> Option<ScopeId> {
        self.current_scope()
    }

    fn is_budget_exhausted(&self) -> bool {
        self.budget.exhausted()
    }

    fn visit_ident(&mut self, ident: &Ident) {
        self.budget.try_charge();
        if self.names.intern(ident.sym.as_ref()).is_err() {
            self.name_exhausted = true;
        }
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if let Some(property) = crate::analysis::syntax::literal_member_property_name(&member.prop)
        {
            self.budget.try_charge();
            if self.names.intern(property.as_str()).is_err() {
                self.name_exhausted = true;
            }
        }
    }

    fn visit_prop_name(&mut self, property: &PropName) {
        if let Some(property) = crate::analysis::syntax::literal_property_name(property) {
            self.budget.try_charge();
            if self.names.intern(property.as_str()).is_err() {
                self.name_exhausted = true;
            }
        }
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        if let Some(scope) = self.current_scope() {
            self.insert_import(scope, import);
        }
    }

    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        let Some(scope) = self.binding_scope(declaration.kind) else {
            return;
        };
        for declarator in &declaration.decls {
            self.insert_pat_locals(scope, &declarator.name);
        }
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        if let Some(scope) = self.current_scope() {
            self.insert_local(scope, declaration.ident.sym.to_string());
        }
    }

    fn before_fn_decl(&mut self, declaration: &FnDecl, parent: ScopeId) {
        self.insert_local(parent, declaration.ident.sym.to_string());
    }

    fn after_fn_decl(&mut self, declaration: &FnDecl, scope: ScopeId) {
        for parameter in &declaration.function.params {
            self.insert_pat_locals(scope, &parameter.pat);
        }
    }

    fn after_function(&mut self, function: &Function, scope: ScopeId) {
        for parameter in &function.params {
            self.insert_pat_locals(scope, &parameter.pat);
        }
    }

    fn after_arrow(&mut self, arrow: &ArrowExpr, scope: ScopeId) {
        for parameter in &arrow.params {
            self.insert_pat_locals(scope, parameter);
        }
    }
}
