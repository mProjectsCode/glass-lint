use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_ecma_ast::{ArrowExpr, Expr, Function, ImportDecl, Pat, VarDeclKind};

use crate::analysis::{
    SemanticBudget,
    scope::{
        BindingProvenance, ScopeId, ScopeKind, ScopedName,
        build::{
            ScopeCollectionArtifacts, ScopeCollector,
            bindings::{for_each_import_binding, for_each_pat_binding, var_binding_scope},
            compact_pat::{CompactPat, compact_pat},
            plan::ScopePlan,
            program::ScopeCollectionIssue,
        },
        query::rooted::RootedExprContext,
    },
    syntax::{
        function_prototype_builtin, is_function_constructor_member, literal_member_property_name,
    },
};

impl ScopeCollector<'_> {
    #[cfg(test)]
    pub(crate) fn from_plan_for_test(plan: ScopePlan) -> ScopeCollector<'static> {
        Self::from_plan(plan, Box::leak(Box::new(SemanticBudget::default())))
    }

    pub(crate) fn from_plan(plan: ScopePlan, budget: &SemanticBudget) -> ScopeCollector<'_> {
        ScopeCollector {
            lexical: super::LexicalCollectionState {
                scopes: plan.scopes,
                stack: vec![0],
                names: plan.names,
                name_exhausted: plan.name_exhausted,
                scope_shapes: plan.scope_shapes,
            },
            assignment: super::AssignmentCollectionState::default(),
            artifacts: ScopeCollectionArtifacts::default(),
            functions: super::FunctionCollectionState::default(),
            budget,
            #[cfg(test)]
            scope_lookups: 0,
        }
    }

    pub(super) fn current_scope(&self) -> ScopeId {
        ScopeId::new(self.lexical.stack.last().copied().unwrap_or(0))
    }

    pub(super) fn binding_scope(&self, kind: VarDeclKind) -> ScopeId {
        if kind != VarDeclKind::Var {
            return self.current_scope();
        }
        var_binding_scope(&self.lexical.stack, &self.lexical.scopes)
    }

    pub fn insert(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: BindingProvenance,
    ) {
        let name = name.into();
        self.budget.try_charge();
        let Some(name) = self.lookup_or_intern_name(name.as_str()) else {
            self.lexical.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        if let Some(scope_data) = self.lexical.scopes.get_mut(scope) {
            scope_data.insert_binding(name, provenance);
        }
    }

    pub(super) fn intern_provenance_strings(&mut self, provenance: &BindingProvenance) {
        match provenance {
            BindingProvenance::StaticString(value) => {
                self.budget.try_charge();
                if self.lexical.names.intern(value.as_str()).is_err() {
                    self.lexical.name_exhausted = true;
                }
            }
            BindingProvenance::StaticStringArray(values) => {
                for value in values {
                    self.budget.try_charge();
                    if self.lexical.names.intern(value.as_str()).is_err() {
                        self.lexical.name_exhausted = true;
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn insert_import(&mut self, scope: ScopeId, import: &ImportDecl) {
        for_each_import_binding(import, |name, provenance| {
            self.insert(scope, name, provenance);
        });
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

    pub(super) fn interned_name(&self, name: &str) -> Option<NameId> {
        self.lexical.names.lookup(name)
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

    pub(super) fn insert_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    pub(super) fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| self.insert_local(scope, binding));
    }

    pub(super) fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) -> bool {
        let parent = self.current_scope();
        if let Some(scope_id) = self
            .lexical
            .scope_shapes
            .take_child(Some(parent), span.lo, kind)
        {
            self.lexical.stack.push(scope_id.index());
            #[cfg(test)]
            {
                self.scope_lookups += 1;
            }
            true
        } else {
            self.artifacts
                .record_issue(ScopeCollectionIssue::ShapeMismatch);
            false
        }
    }

    pub(super) fn pop_scope(&mut self) {
        if self.lexical.stack.len() <= 1 {
            debug_assert!(false, "attempted to pop the program scope");
            return;
        }
        let _ = self.lexical.stack.pop();
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
