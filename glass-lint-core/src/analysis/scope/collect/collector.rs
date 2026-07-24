use hashbrown::{HashMap, HashSet};

use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;
use swc_ecma_ast::{ArrowExpr, Expr, Function, ImportDecl, Pat, VarDeclKind};

use crate::{
    analysis::{
        SemanticBudget,
        scope::{
            BindingProvenance, ScopeId, ScopeKind, ScopedName,
            query::rooted::RootedExprContext,
        },
        syntax::{
            function_prototype_builtin, is_function_constructor_member, member_property_name,
        },
    },
};

use super::{
    history::AssignmentHistory,
    plan::ScopePlan,
    compact_pat::{CompactPat, compact_pat},
    program::ScopeCollectionIssue,
    bindings::{for_each_import_binding, for_each_pat_binding, var_binding_scope},
    ScopeCollector,
};

impl ScopeCollector<'_> {
    #[cfg(test)]
    pub(crate) fn from_plan_for_test(plan: ScopePlan) -> ScopeCollector<'static> {
        Self::from_plan(plan, Box::leak(Box::new(SemanticBudget::default())))
    }

    pub(crate) fn from_plan(plan: ScopePlan, budget: &SemanticBudget) -> ScopeCollector<'_> {
        ScopeCollector {
            scopes: plan.scopes,
            stack: vec![0],
            assignments: Vec::new(),
            latest_assignments: AssignmentHistory::new(),
            property_assignments: Vec::new(),
            rooted_property_mutations: Vec::new(),
            dynamic_evals: Vec::new(),
            function_scopes: HashMap::new(),
            function_aliases: HashMap::new(),
            calls: Vec::new(),
            inline_parameters: HashMap::new(),
            mutable_static_objects: HashSet::new(),
            pending_function_names: HashMap::new(),
            names: plan.names,
            name_exhausted: plan.name_exhausted,
            version_counters: HashMap::new(),
            scope_shapes: plan.scope_shapes,
            scope_issues: Vec::new(),
            budget,
            #[cfg(test)]
            scope_lookups: 0,
        }
    }

    pub(super) fn current_scope(&self) -> ScopeId {
        ScopeId::from(self.stack.last().copied().unwrap_or(0))
    }

    pub(super) fn is_module_interop_wrapper(name: &str) -> bool {
        matches!(
            name,
            "__toESM"
                | "__importStar"
                | "__importDefault"
                | "_interopRequireWildcard"
                | "_interopRequireDefault"
        )
    }

    pub(super) fn binding_scope(&self, kind: VarDeclKind) -> ScopeId {
        if kind != VarDeclKind::Var {
            return self.current_scope();
        }
        var_binding_scope(&self.stack, &self.scopes)
    }

    pub fn insert(
        &mut self,
        scope: ScopeId,
        name: impl Into<SmolStr>,
        provenance: BindingProvenance,
    ) {
        let name = name.into();
        self.budget.try_charge();
        let Ok(name) = self.names.intern(name.as_str()) else {
            self.name_exhausted = true;
            return;
        };
        self.intern_provenance_strings(&provenance);
        self.scopes[scope.index()].bindings.insert(name, provenance);
    }

    pub(super) fn intern_provenance_strings(&mut self, provenance: &BindingProvenance) {
        match provenance {
            BindingProvenance::StaticString(value) => {
                self.budget.try_charge();
                if self.names.intern(value.as_str()).is_err() {
                    self.name_exhausted = true;
                }
            }
            BindingProvenance::StaticStringArray(values) => {
                for value in values {
                    self.budget.try_charge();
                    if self.names.intern(value.as_str()).is_err() {
                        self.name_exhausted = true;
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
        self.names.lookup(name)
    }

    pub(super) fn interned_name(&self, name: &str) -> Option<NameId> {
        self.names.lookup(name)
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.names.lookup_path(path)
    }

    pub(super) fn rooted_name_path(&self, expr: &Expr) -> Option<NamePath> {
        self.rooted_expr_name(expr)
            .and_then(|path| self.name_path(&path))
    }

    pub(super) fn append_name_path(&self, path: &NamePath, segment: &str) -> Option<NamePath> {
        let id = self.names.lookup(segment)?;
        Some(path.append_path(&NamePath::from_ids([id])))
    }

    pub(super) fn scoped_name(&self, scope: ScopeId, name: &str) -> Option<ScopedName> {
        self.names
            .lookup(name)
            .map(|name| ScopedName::new(scope, name))
    }

    pub(super) fn scoped_name_by_id(scope: ScopeId, name: NameId) -> ScopedName {
        ScopedName::new(scope, name)
    }

    pub(super) fn insert_local(&mut self, scope: ScopeId, name: impl Into<SmolStr>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    pub(super) fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        for_each_pat_binding(pat, |binding| self.insert_local(scope, binding));
    }

    pub(super) fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) {
        let parent = self.current_scope();
        if let Some(scope_id) = self.scope_shapes.take_child(Some(parent), span.lo, kind) {
            self.stack.push(scope_id.index());
            #[cfg(test)]
            {
                self.scope_lookups += 1;
            }
        } else {
            self.scope_issues.push(ScopeCollectionIssue::ShapeMismatch);
        }
    }

    pub(super) fn pop_scope(&mut self) {
        if self.stack.len() <= 1 {
            debug_assert!(false, "attempted to pop the program scope");
            return;
        }
        let _ = self.stack.pop();
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
            ) => self.names.resolve_path(target),
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
        let property = member_property_name(&member.prop)?;
        Some(object.append_chain(&property))
    }
}
