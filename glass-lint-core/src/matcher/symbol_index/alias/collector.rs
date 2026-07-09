use std::collections::{BTreeMap, BTreeSet};

use swc_common::Span;
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, BlockStmt, CallExpr, Callee, CatchClause, ClassDecl, Expr,
    FnDecl, Function, ImportDecl, ImportSpecifier, Lit, Pat, SimpleAssignTarget, VarDecl,
    VarDeclKind,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::ast::{
    binding_ident_name, collect_pat_bindings, is_function_constructor_member, member_chain,
    member_prop_name, module_export_name, object_keys, static_string,
};
use super::collector_helpers::{
    collect_assignment_aliases, collect_require_aliases, collect_value_aliases,
};
use super::{
    AliasAssignment, AliasScope, BindingProvenance, PropertyAliasAssignment, RootedExprContext,
    ScopeKind, rooted_expr_chain_with,
};

pub struct AliasCollector {
    pub scopes: Vec<AliasScope>,
    stack: Vec<usize>,
    pub assignments: Vec<AliasAssignment>,
    latest_assignments: BTreeMap<usize, BTreeMap<String, BindingProvenance>>,
    pub property_assignments: Vec<PropertyAliasAssignment>,
    functions: BTreeMap<String, (usize, Vec<String>)>,
    calls: Vec<(String, Vec<Option<BindingProvenance>>)>,
}

fn is_module_interop_wrapper(name: &str) -> bool {
    matches!(
        name,
        "__toESM"
            | "__importStar"
            | "__importDefault"
            | "_interopRequireWildcard"
            | "_interopRequireDefault"
    )
}

impl AliasCollector {
    pub fn new(program_span: Span) -> Self {
        Self {
            scopes: vec![AliasScope {
                span: program_span,
                depth: 0,
                kind: ScopeKind::Program,
                parent: None,
                bindings: BTreeMap::new(),
            }],
            stack: vec![0],
            assignments: Vec::new(),
            latest_assignments: BTreeMap::new(),
            property_assignments: Vec::new(),
            functions: BTreeMap::new(),
            calls: Vec::new(),
        }
    }

    fn current_scope(&self) -> usize {
        *self.stack.last().expect("program scope is always present")
    }

    fn binding_scope(&self, kind: VarDeclKind) -> usize {
        if kind != VarDeclKind::Var {
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
            .expect("program scope is always present")
    }

    pub fn insert(&mut self, scope: usize, name: impl Into<String>, provenance: BindingProvenance) {
        self.scopes[scope].bindings.insert(name.into(), provenance);
    }

    fn insert_local(&mut self, scope: usize, name: impl Into<String>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    pub fn record_assignment(
        &mut self,
        span: Span,
        scope: usize,
        name: String,
        provenance: BindingProvenance,
    ) {
        self.latest_assignments
            .entry(scope)
            .or_default()
            .insert(name.clone(), provenance.clone());
        self.assignments.push(AliasAssignment {
            span,
            scope,
            name,
            provenance,
        });
    }

    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        let index = self.scopes.len();
        let parent = self.current_scope();
        self.scopes.push(AliasScope {
            span,
            depth: self.stack.len(),
            kind,
            parent: Some(parent),
            bindings: BTreeMap::new(),
        });
        self.stack.push(index);
    }

    fn pop_scope(&mut self) {
        self.stack.pop();
    }

    fn insert_pat_locals(&mut self, scope: usize, pat: &Pat) {
        let mut bindings = BTreeSet::new();
        collect_pat_bindings(pat, &mut bindings);
        for binding in bindings {
            self.insert_local(scope, binding);
        }
    }

    fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        for scope in self.stack.iter().rev().copied() {
            if let Some(assignment) = self
                .latest_assignments
                .get(&scope)
                .and_then(|assignments| assignments.get(name))
            {
                return Some(assignment);
            }
            if let Some(binding) = self.scopes[scope].bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn is_unbound(&self, name: &str) -> bool {
        self.visible_binding(name).is_none()
    }

    fn rooted_expr_name(&self, expr: &Expr) -> Option<String> {
        rooted_expr_chain_with(self, expr)
    }

    fn module_alias_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        match expr {
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                provenance @ (BindingProvenance::ModuleExport { .. }
                | BindingProvenance::ModuleNamespace { .. }) => Some(provenance.clone()),
                BindingProvenance::Local
                | BindingProvenance::ValueAlias { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_) => None,
            },
            Expr::Member(member) => {
                let BindingProvenance::ModuleNamespace { module } =
                    self.module_alias_provenance(&member.obj)?
                else {
                    return None;
                };
                Some(BindingProvenance::ModuleExport {
                    module: module.clone(),
                    export: member_prop_name(&member.prop)?,
                })
            }
            Expr::Call(call) => self
                .require_module_name(call)
                .map(|module| BindingProvenance::ModuleNamespace { module }),
            Expr::Paren(paren) => self.module_alias_provenance(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.module_alias_provenance(expr)),
            _ => None,
        }
    }

    fn require_module_name(&self, call: &CallExpr) -> Option<String> {
        self.direct_require_module_name(call).or_else(|| {
            let Callee::Expr(callee) = &call.callee else {
                return None;
            };
            let Expr::Ident(wrapper) = &**callee else {
                return None;
            };
            (is_module_interop_wrapper(wrapper.sym.as_ref())
                && self.is_unbound(wrapper.sym.as_ref()))
            .then(|| call.args.first())
            .flatten()
            .and_then(|arg| self.require_module_expr_name(&arg.expr))
        })
    }

    fn require_module_expr_name(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call(call) => self.require_module_name(call),
            Expr::Member(member) => self.require_module_expr_name(&member.obj),
            Expr::Paren(paren) => self.require_module_expr_name(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.require_module_expr_name(expr)),
            _ => None,
        }
    }

    fn direct_require_module_name(&self, call: &CallExpr) -> Option<String> {
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Ident(ident) = &**callee else {
            return None;
        };
        if ident.sym != *"require" || !self.is_unbound("require") {
            return None;
        }
        call.args.first().and_then(|arg| match &*arg.expr {
            Expr::Lit(Lit::Str(value)) => Some(value.value.to_string_lossy().to_string()),
            _ => None,
        })
    }

    fn const_provenance(&self, init: &Expr) -> Option<BindingProvenance> {
        if let Some(value) = static_string(init) {
            return Some(BindingProvenance::StaticString(value));
        }
        if let Expr::Array(array) = init {
            let values = array
                .elems
                .iter()
                .map(|elem| elem.as_ref().and_then(|elem| static_string(&elem.expr)))
                .collect::<Option<Vec<_>>>()?;
            return Some(BindingProvenance::StaticStringArray(values));
        }
        if let Expr::Object(object) = init
            && let Some(keys) = object_keys(object)
        {
            return Some(BindingProvenance::StaticObjectKeys(keys));
        }
        None
    }

    fn function_parameters(function: &Function) -> Vec<String> {
        function
            .params
            .iter()
            .filter_map(|parameter| binding_ident_name(&parameter.pat))
            .collect()
    }

    fn arrow_parameters(arrow: &ArrowExpr) -> Vec<String> {
        arrow.params.iter().filter_map(binding_ident_name).collect()
    }

    fn register_function_expression(&mut self, name: String, expr: &Expr) -> bool {
        match expr {
            Expr::Arrow(arrow) => {
                let parameters = Self::arrow_parameters(arrow);
                self.push_scope(arrow.span, ScopeKind::Function);
                let scope = self.current_scope();
                for param in &arrow.params {
                    self.insert_pat_locals(scope, param);
                }
                self.functions.insert(name, (scope, parameters));
                arrow.body.visit_with(self);
                self.pop_scope();
                true
            }
            Expr::Fn(function_expr) => {
                let parameters = Self::function_parameters(&function_expr.function);
                self.push_scope(function_expr.function.span, ScopeKind::Function);
                let scope = self.current_scope();
                if let Some(ident) = &function_expr.ident {
                    self.insert_local(scope, ident.sym.to_string());
                }
                for param in &function_expr.function.params {
                    self.insert_pat_locals(scope, &param.pat);
                }
                self.functions.insert(name, (scope, parameters));
                function_expr.function.decorators.visit_with(self);
                function_expr.function.body.visit_with(self);
                self.pop_scope();
                true
            }
            Expr::Paren(paren) => self.register_function_expression(name, &paren.expr),
            _ => false,
        }
    }

    pub fn parameter_aliases(&self) -> BTreeMap<(usize, String), BindingProvenance> {
        let mut aliases = BTreeMap::<(usize, String), Option<BindingProvenance>>::new();
        for (callee, arguments) in &self.calls {
            let Some((scope, parameters)) = self.functions.get(callee) else {
                continue;
            };
            for (parameter, target) in parameters.iter().zip(arguments) {
                let entry = aliases
                    .entry((*scope, parameter.clone()))
                    .or_insert_with(|| target.clone());
                if entry != target {
                    *entry = None;
                }
            }
        }
        aliases
            .into_iter()
            .filter_map(|(key, target)| target.map(|target| (key, target)))
            .collect()
    }
}

impl RootedExprContext for AliasCollector {
    fn rooted_ident_chain(&self, ident: &swc_ecma_ast::Ident) -> Option<String> {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(BindingProvenance::ValueAlias { target }) => Some(target.clone()),
            Some(_) => None,
            None => Some(ident.sym.to_string()),
        }
    }

    fn rooted_member_chain(&self, member: &swc_ecma_ast::MemberExpr) -> Option<String> {
        if is_function_constructor_member(member) {
            return Some("Function".to_string());
        }
        let object = self.rooted_expr_name(&member.obj)?;
        let property = member_prop_name(&member.prop)?;
        Some(format!("{object}.{property}"))
    }
}

impl Visit for AliasCollector {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let scope = self.current_scope();
        let module = import.src.value.to_string_lossy().to_string();
        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_string();
                    let export = named
                        .imported
                        .as_ref()
                        .map(module_export_name)
                        .unwrap_or_else(|| local.clone());
                    self.insert(
                        scope,
                        local,
                        BindingProvenance::ModuleExport {
                            module: module.clone(),
                            export,
                        },
                    );
                }
                ImportSpecifier::Namespace(namespace) => self.insert(
                    scope,
                    namespace.local.sym.to_string(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
                ImportSpecifier::Default(default) => {
                    self.insert_local(scope, default.local.sym.to_string());
                }
            }
        }
    }

    fn visit_var_decl(&mut self, var_decl: &VarDecl) {
        let scope = self.binding_scope(var_decl.kind);
        for declarator in &var_decl.decls {
            if let (Pat::Ident(ident), Some(init)) = (&declarator.name, declarator.init.as_deref())
                && self.register_function_expression(ident.id.sym.to_string(), init)
            {
                self.insert_local(scope, ident.id.sym.to_string());
                continue;
            }
            let init = declarator.init.as_deref();
            let module_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.module_alias_provenance(init));
            let value_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.rooted_expr_name(init));
            let const_value = declarator
                .init
                .as_deref()
                .and_then(|init| self.const_provenance(init));
            self.insert_pat_locals(scope, &declarator.name);
            if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, module_alias) {
                self.insert(scope, ident.id.sym.to_string(), provenance);
            } else if let Some(init) = declarator.init.as_deref()
                && let Some(module) = self.require_module_expr_name(init)
            {
                collect_require_aliases(&declarator.name, module, scope, self);
            } else if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, const_value) {
                self.insert(scope, ident.id.sym.to_string(), provenance);
            } else if let Some(target) = value_alias {
                collect_value_aliases(&declarator.name, &target, scope, self);
            }
            if let Some(init) = init {
                init.visit_with(self);
            }
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let provenance = self
            .module_alias_provenance(&assignment.right)
            .or_else(|| self.const_provenance(&assignment.right))
            .or_else(|| {
                self.rooted_expr_name(&assignment.right)
                    .map(|target| BindingProvenance::ValueAlias { target })
            })
            .unwrap_or(BindingProvenance::Local);
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                if let Some((scope, _)) = self.stack.iter().rev().find_map(|scope| {
                    self.scopes[*scope]
                        .bindings
                        .contains_key(ident.id.sym.as_ref())
                        .then_some((*scope, ()))
                }) {
                    self.record_assignment(
                        assignment.span,
                        scope,
                        ident.id.sym.to_string(),
                        provenance,
                    );
                }
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                if let Some(property) = member_chain(member) {
                    self.property_assignments.push(PropertyAliasAssignment {
                        span: assignment.span,
                        scope: self.current_scope(),
                        property,
                        target: self.rooted_expr_name(&assignment.right),
                    });
                }
            }
            AssignTarget::Pat(pattern) => {
                let pattern: Pat = pattern.clone().into();
                if let Some(target) = self.rooted_expr_name(&assignment.right) {
                    collect_assignment_aliases(
                        &pattern,
                        &target,
                        assignment.span,
                        self.current_scope(),
                        self,
                    );
                }
            }
            _ => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Ident(callee) = &**callee
        {
            self.calls.push((
                callee.sym.to_string(),
                call.args
                    .iter()
                    .map(|argument| {
                        self.module_alias_provenance(&argument.expr)
                            .or_else(|| self.const_provenance(&argument.expr))
                            .or_else(|| {
                                self.rooted_expr_name(&argument.expr)
                                    .map(|target| BindingProvenance::ValueAlias { target })
                            })
                    })
                    .collect(),
            ));
        }
        call.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, fn_decl: &FnDecl) {
        let parent = self.current_scope();
        self.insert_local(parent, fn_decl.ident.sym.to_string());
        self.push_scope(fn_decl.function.span, ScopeKind::Function);
        let scope = self.current_scope();
        let parameters = Self::function_parameters(&fn_decl.function);
        for parameter in &fn_decl.function.params {
            self.insert_pat_locals(scope, &parameter.pat);
        }
        self.functions
            .insert(fn_decl.ident.sym.to_string(), (scope, parameters));
        fn_decl.function.decorators.visit_with(self);
        fn_decl.function.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        let scope = self.current_scope();
        self.insert_local(scope, class_decl.ident.sym.to_string());
        class_decl.class.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        self.push_scope(function.span, ScopeKind::Function);
        let scope = self.current_scope();
        for param in &function.params {
            self.insert_pat_locals(scope, &param.pat);
        }
        function.decorators.visit_with(self);
        function.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.push_scope(arrow.span, ScopeKind::Function);
        let scope = self.current_scope();
        for param in &arrow.params {
            self.insert_pat_locals(scope, param);
        }
        arrow.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.push_scope(block.span, ScopeKind::Block);
        block.stmts.visit_with(self);
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, catch: &CatchClause) {
        self.push_scope(catch.span, ScopeKind::Block);
        let scope = self.current_scope();
        if let Some(param) = &catch.param {
            self.insert_pat_locals(scope, param);
        }
        catch.body.stmts.visit_with(self);
        self.pop_scope();
    }
}
