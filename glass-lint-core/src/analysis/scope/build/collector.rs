use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_ecma_ast::{ArrowExpr, Expr, Function, Pat, VarDeclKind};

use crate::analysis::{
    SemanticBudget,
    scope::{
        BindingProvenance, ScopeId, ScopedName,
        build::{
            ScopeCollectionArtifacts, ScopeCollector,
            bindings::{
                for_each_pat_binding, intern_checked, register_declaration_binding,
                var_binding_scope,
            },
            compact_pat::{CompactPat, compact_pat},
            plan::ScopePlan,
            traversal::ScopePass,
        },
        query::rooted::RootedExprContext,
    },
    syntax::{
        function_prototype_builtin, is_function_constructor_member, literal_member_property_name,
    },
};

impl ScopeCollector<'_> {
    pub(crate) fn from_plan(plan: ScopePlan, budget: &SemanticBudget) -> ScopeCollector<'_> {
        let ScopePlan {
            program,
            scopes,
            names,
            name_exhausted,
            scope_shapes,
        } = plan;
        ScopeCollector {
            lexical: super::LexicalCollectionState {
                scopes,
                stack: vec![program],
                names,
                name_exhausted,
                scope_shapes,
            },
            assignment: super::AssignmentCollectionState::default(),
            artifacts: ScopeCollectionArtifacts::default(),
            functions: super::FunctionCollectionState::default(),
            budget,
            #[cfg(test)]
            scope_lookups: 0,
        }
    }

    pub(super) fn binding_scope(&self, kind: VarDeclKind) -> Option<ScopeId> {
        if self.artifacts.has_issues() {
            return None;
        }
        if kind != VarDeclKind::Var {
            return self.current_scope();
        }
        var_binding_scope(&self.lexical.stack, &self.lexical.scopes)
    }

    pub(super) fn register_binding(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: &BindingProvenance,
    ) {
        register_declaration_binding(
            &mut self.lexical.scopes,
            &mut self.lexical.names,
            &mut self.lexical.name_exhausted,
            self.budget,
            scope,
            name,
            provenance.clone(),
        );
        self.intern_provenance_strings(provenance);
    }

    pub(super) fn update_binding(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: BindingProvenance,
    ) {
        let name = name.into();
        self.intern_provenance_strings(&provenance);
        let Some(name) = self.name_id(name.as_str()) else {
            self.lexical.name_exhausted = true;
            return;
        };
        if let Some(scope_data) = self.lexical.scopes.get_mut(scope) {
            scope_data.update_binding(name, provenance);
        }
    }

    pub(super) fn intern_provenance_strings(&mut self, provenance: &BindingProvenance) {
        match provenance {
            BindingProvenance::StaticString(value) => {
                intern_checked(
                    &mut self.lexical.names,
                    &mut self.lexical.name_exhausted,
                    self.budget,
                    value.as_str(),
                );
            }
            BindingProvenance::StaticStringArray(values) => {
                for value in values {
                    intern_checked(
                        &mut self.lexical.names,
                        &mut self.lexical.name_exhausted,
                        self.budget,
                        value.as_str(),
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn name_id(&self, name: &str) -> Option<NameId> {
        self.lexical.names.lookup(name)
    }

    pub(super) fn lookup_or_intern_name(&mut self, name: &str) -> Option<NameId> {
        self.lexical
            .names
            .lookup(name)
            .or_else(|| self.lexical.names.intern(name).ok())
    }

    pub(super) fn name_path(&mut self, path: &SymbolPath) -> Option<NamePath> {
        let mut resolved = NamePath::new();
        for segment in path.segments() {
            let id = self.lookup_or_intern_name(segment.as_str())?;
            resolved.append(id);
        }
        Some(resolved)
    }

    pub(super) fn rooted_name_path(&mut self, expr: &Expr) -> Option<NamePath> {
        self.rooted_expr_name(expr)
            .and_then(|path| self.name_path(&path))
    }

    pub(super) fn append_name_path(&mut self, path: &NamePath, segment: &str) -> Option<NamePath> {
        let id = self.lookup_or_intern_name(segment)?;
        Some(path.append_path(&NamePath::from_ids([id])))
    }

    pub(super) fn scoped_name(&self, scope: ScopeId, name: &str) -> Option<ScopedName> {
        self.lexical
            .names
            .lookup(name)
            .map(|name| ScopedName::new(scope, name))
    }

    pub(super) fn register_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.register_binding(scope, name, &BindingProvenance::Local);
    }

    pub(super) fn register_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| self.register_local(scope, binding));
    }

    pub(super) fn reset_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| {
            self.update_binding(scope, binding, BindingProvenance::Local);
        });
    }

    pub(super) fn function_parameters(function: &Function) -> Vec<CompactPat> {
        function
            .params
            .iter()
            .map(|parameter| compact_pat(&parameter.pat))
            .collect()
    }

    pub(super) fn arrow_parameters(arrow: &ArrowExpr) -> Vec<CompactPat> {
        arrow.params.iter().map(compact_pat).collect()
    }
}

impl RootedExprContext for ScopeCollector<'_> {
    fn rooted_ident_chain(&self, ident: &swc_ecma_ast::Ident) -> Option<SymbolPath> {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. },
            ) => self.lexical.names.resolve_path(target),
            Some(_) => None,
            None => Some(ident.sym.as_ref().into()),
        }
    }

    fn rooted_member_chain(&self, member: &swc_ecma_ast::MemberExpr) -> Option<SymbolPath> {
        if is_function_constructor_member(member)
            && function_prototype_builtin(&member.obj).is_none_or(|name| self.is_unbound(name))
        {
            return Some("Function".into());
        }
        let object = self.rooted_expr_name(&member.obj)?;
        let property = literal_member_property_name(&member.prop)?;
        Some(object.append_chain(&property))
    }
}
