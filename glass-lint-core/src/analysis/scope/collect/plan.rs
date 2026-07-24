//! Declaration-only lexical scope planning.
//!
//! The planner establishes binding visibility and the structural identity of
//! every scope-forming node. It deliberately does not record assignments,
//! aliases, calls, or other source-order facts.

use std::collections::BTreeMap;

use glass_lint_datastructures::NameTable;
use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{
    ArrowExpr, ClassDecl, FnDecl, Function, Ident, ImportDecl, MemberExpr, Pat, PropName, VarDecl,
};

use super::{ScopeShape, ScopeShapeTable, traversal::ScopePass};
use crate::analysis::{
    SemanticBudget,
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

pub(in crate::analysis::scope) struct ScopePlanner<'a> {
    names: NameTable,
    scopes: Vec<LexicalScope>,
    stack: Vec<usize>,
    scope_shapes: ScopeShapeTable,
    name_exhausted: bool,
    budget: &'a SemanticBudget,
}

impl ScopePlanner<'_> {
    #[cfg(test)]
    pub(in crate::analysis::scope) fn new_for_test(
        program_span: swc_common::Span,
        names: NameTable,
    ) -> ScopePlanner<'static> {
        Self::new(
            program_span,
            names,
            Box::leak(Box::new(SemanticBudget::default())),
        )
    }

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
        ScopePlanner {
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
            budget,
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
        self.budget.try_charge();
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

impl ScopePass for ScopePlanner<'_> {
    fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) {
        self.push_scope(span, kind);
    }

    fn pop_scope(&mut self) {
        self.pop_scope();
    }

    fn current_scope(&self) -> ScopeId {
        self.current_scope()
    }

    fn visit_ident(&mut self, ident: &Ident) {
        self.budget.try_charge();
        if self.names.intern(ident.sym.as_ref()).is_err() {
            self.name_exhausted = true;
        }
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if let Some(property) = crate::analysis::syntax::member_property_name(&member.prop) {
            self.budget.try_charge();
            if self.names.intern(property.as_str()).is_err() {
                self.name_exhausted = true;
            }
        }
    }

    fn visit_prop_name(&mut self, property: &PropName) {
        if let Some(property) = crate::analysis::syntax::property_name(property) {
            self.budget.try_charge();
            if self.names.intern(property.as_str()).is_err() {
                self.name_exhausted = true;
            }
        }
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.insert_import(self.current_scope(), import);
    }

    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        let scope = self.binding_scope(declaration.kind);
        for declarator in &declaration.decls {
            self.insert_pat_locals(scope, &declarator.name);
        }
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        self.insert_local(self.current_scope(), declaration.ident.sym.to_string());
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
