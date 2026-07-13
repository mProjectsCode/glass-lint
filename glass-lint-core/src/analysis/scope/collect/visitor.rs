use super::{
    AliasCollector, ArrowExpr, AssignExpr, AssignTarget, BindingProvenance, BlockStmt, CallExpr,
    Callee, CatchClause, ClassDecl, Expr, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function,
    ImportDecl, ImportSpecifier, ObjectPatProp, Pat, PropertyAliasAssignment,
    RootedPropertyMutation, ScopeKind, SimpleAssignTarget, Spanned, SwitchStmt, VarDecl,
    VarDeclKind, Visit, VisitWith, WithStmt, function_prototype_builtin, member_chain,
    member_prop_name, member_root_ident, module_export_name, prop_name,
};

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
                ImportSpecifier::Namespace(namespace) => self.insert(
                    scope,
                    namespace.local.sym.to_string(),
                    BindingProvenance::ModuleNamespace {
                        module: module.clone(),
                    },
                ),
                ImportSpecifier::Default(default) => {
                    self.insert(
                        scope,
                        default.local.sym.to_string(),
                        BindingProvenance::ModuleNamespace {
                            module: module.clone(),
                        },
                    );
                }
            }
        }
    }

    fn visit_var_decl(&mut self, var_decl: &VarDecl) {
        let scope = self.binding_scope(var_decl.kind);
        for declarator in &var_decl.decls {
            let mutable_object = var_decl.kind == VarDeclKind::Var
                && matches!(
                    declarator.init.as_deref().and_then(|init| {
                        self.static_object_values(init)
                            .or_else(|| self.const_provenance(init))
                    }),
                    Some(
                        BindingProvenance::StaticObjectKeys(_)
                            | BindingProvenance::StaticObjectValues(_)
                    )
                );
            if mutable_object && let Pat::Ident(ident) = &declarator.name {
                self.mutable_static_objects
                    .insert((scope, ident.id.sym.to_string()));
            }
            if let (Pat::Ident(ident), Some(init)) = (&declarator.name, declarator.init.as_deref())
                && self.register_function_expression(ident.id.sym.to_string(), init)
            {
                self.insert_local(scope, ident.id.sym.to_string());
                continue;
            }
            if let (Pat::Ident(alias), Some(Expr::Ident(target))) =
                (&declarator.name, declarator.init.as_deref())
                && let Some(function_scope) = self.function_scope_for_name(target.sym.as_ref())
            {
                self.function_aliases
                    .insert((scope, alias.id.sym.to_string()), function_scope);
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
            let function_constructor_alias = value_alias
                .as_deref()
                .filter(|target| *target == "Function")
                .map(|target| BindingProvenance::ValueAlias {
                    target: target.to_string().into(),
                });
            let returned_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.returned_object_provenance(init));
            let const_value = declarator.init.as_deref().and_then(|init| {
                self.static_object_values(init)
                    .or_else(|| self.const_provenance(init))
            });
            let bound_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.bound_callable_provenance(init));
            self.insert_pat_locals(scope, &declarator.name);
            let derived_function_pattern = if let (Pat::Object(object), Some(init)) =
                (&declarator.name, init)
                && function_prototype_builtin(init).is_some_and(|name| self.is_unbound(name))
            {
                for property in &object.props {
                    if let ObjectPatProp::KeyValue(property) = property
                        && prop_name(&property.key).as_deref() == Some("constructor")
                    {
                        self.collect_value_aliases(&property.value, "Function", scope);
                    }
                }
                true
            } else {
                false
            };
            if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, bound_alias.as_ref())
            {
                self.insert(scope, ident.id.sym.to_string(), provenance.clone());
            } else if let (Pat::Ident(ident), Some(provenance)) =
                (&declarator.name, module_alias.as_ref())
            {
                self.insert(scope, ident.id.sym.to_string(), provenance.clone());
            } else if let Some(BindingProvenance::ModuleNamespace { module }) =
                module_alias.as_ref()
            {
                self.collect_require_aliases(&declarator.name, module.clone(), scope);
            } else if let Some(init) = declarator.init.as_deref()
                && let Some(module) = self.require_module_expr_name(init)
            {
                self.collect_require_aliases(&declarator.name, module, scope);
            } else if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, const_value) {
                self.insert(scope, ident.id.sym.to_string(), provenance);
            } else if let (Pat::Ident(ident), Some(provenance)) =
                (&declarator.name, function_constructor_alias)
            {
                self.insert(scope, ident.id.sym.to_string(), provenance);
            } else if value_alias
                .as_deref()
                .is_none_or(|target| target.contains('.'))
                && let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, returned_alias)
            {
                self.insert(scope, ident.id.sym.to_string(), provenance);
            } else if !derived_function_pattern && let Some(target) = value_alias {
                self.collect_value_aliases(&declarator.name, &target, scope);
            }
            if let Some(init) = init {
                init.visit_with(self);
            }
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let rooted_alias = self.rooted_expr_name(&assignment.right);
        let function_constructor_alias = rooted_alias
            .as_deref()
            .filter(|target| *target == "Function")
            .map(|target| BindingProvenance::ValueAlias {
                target: target.to_string().into(),
            });
        let provenance = self
            .bound_callable_provenance(&assignment.right)
            .or_else(|| self.module_alias_provenance(&assignment.right))
            .or(function_constructor_alias)
            .or_else(|| self.returned_object_provenance(&assignment.right))
            .or_else(|| self.const_provenance(&assignment.right))
            .or_else(|| {
                rooted_alias.map(|target| BindingProvenance::ValueAlias {
                    target: target.into(),
                })
            })
            .unwrap_or(BindingProvenance::Local);
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                if let Some((scope, ())) = self.stack.iter().rev().find_map(|scope| {
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
                if let Some(receiver) = self.rooted_expr_name(&member.obj) {
                    self.rooted_property_mutations.push(RootedPropertyMutation {
                        span: assignment.span,
                        scope: self.current_scope(),
                        receiver,
                        property: member_prop_name(&member.prop),
                    });
                }
                self.invalidate_member_root(member, assignment.span);
                if let (Some(property), Some(root)) =
                    (member_chain(member), member_root_ident(member))
                {
                    self.property_assignments.push(PropertyAliasAssignment {
                        span: assignment.span,
                        scope: self.current_scope(),
                        property,
                        receiver: root.clone(),
                        target: self.rooted_expr_name(&assignment.right),
                    });
                }
            }
            AssignTarget::Pat(pattern) => {
                let pattern: Pat = pattern.clone().into();
                if let Some(target) = self.rooted_expr_name(&assignment.right) {
                    self.collect_assignment_aliases(
                        &pattern,
                        &target,
                        assignment.span,
                        self.current_scope(),
                    );
                }
            }
            _ => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record_modeled_callbacks(call);
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Ident(callee) = &**callee
        {
            if callee.sym == *"eval" {
                self.dynamic_evals
                    .push((self.binding_scope(VarDeclKind::Var), call.span));
            }
            self.calls.push((
                self.current_scope(),
                callee.sym.to_string(),
                call.args
                    .iter()
                    .map(|argument| self.argument_provenance(&argument.expr))
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
        self.function_scopes
            .insert((parent, fn_decl.ident.sym.to_string()), (scope, parameters));
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
        if let Some(bindings) = self.inline_parameters.get(&function.span.lo).cloned() {
            for (name, provenance) in bindings {
                self.record_assignment(function.span, scope, name, provenance);
            }
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
        if let Some(bindings) = self.inline_parameters.get(&arrow.span.lo).cloned() {
            for (name, provenance) in bindings {
                self.record_assignment(arrow.span, scope, name, provenance);
            }
        }
        arrow.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.push_scope(block.span, ScopeKind::Block);
        block.stmts.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_stmt(&mut self, for_stmt: &ForStmt) {
        self.push_scope(for_stmt.span, ScopeKind::Block);
        for_stmt.init.visit_with(self);
        for_stmt.test.visit_with(self);
        for_stmt.update.visit_with(self);
        for_stmt.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, for_stmt: &ForInStmt) {
        self.push_scope(for_stmt.span, ScopeKind::Block);
        for_stmt.left.visit_with(self);
        for_stmt.right.visit_with(self);
        for_stmt.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_of_stmt(&mut self, for_stmt: &ForOfStmt) {
        self.push_scope(for_stmt.span, ScopeKind::Block);
        for_stmt.left.visit_with(self);
        for_stmt.right.visit_with(self);
        for_stmt.body.visit_with(self);
        self.pop_scope();
    }

    fn visit_switch_stmt(&mut self, switch: &SwitchStmt) {
        switch.discriminant.visit_with(self);
        self.push_scope(switch.span, ScopeKind::Block);
        switch.cases.visit_with(self);
        self.pop_scope();
    }

    fn visit_with_stmt(&mut self, with: &WithStmt) {
        with.obj.visit_with(self);
        self.push_scope(with.body.span(), ScopeKind::Dynamic);
        with.body.visit_with(self);
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
