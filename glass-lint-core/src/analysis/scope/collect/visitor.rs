//! Source-order AST visitor for declarations, assignments, calls, and scopes.
//!
//! The visitor consumes the predeclared scope tree and records only
//! use-position facts that survive lexical shadowing, reassignment, and
//! unsupported dynamic forms.

use smol_str::{SmolStr, ToSmolStr};
use swc_ecma_ast::{CallExpr, Callee, VarDeclarator};

use super::{
    super::super::syntax::property_name, ArrowExpr, AssignExpr, AssignTarget, BindingProvenance,
    BlockStmt, CatchClause, ClassDecl, Expr, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function,
    ImportDecl, ImportSpecifier, LexicalScopeCollector, ObjectPatProp, Pat,
    PropertyAliasAssignment, RootedPropertyMutation, ScopeId, ScopeKind, SimpleAssignTarget,
    Spanned, SwitchStmt, VarDecl, VarDeclKind, Visit, VisitWith, WithStmt,
    function_prototype_builtin, member_expression_chain, member_property_name,
    member_root_identifier, module_export_name,
};
use crate::analysis::value::NamePath;

enum DeclarationClassification {
    Binding {
        name: String,
        provenance: BindingProvenance,
    },
    Require {
        pattern: Pat,
        module: SmolStr,
    },
    ValueAlias {
        pattern: Pat,
        target: NamePath,
    },
    None,
}

#[allow(clippy::too_many_arguments)]
fn classify_declaration(
    collector: &LexicalScopeCollector,
    pattern: &Pat,
    init: Option<&Expr>,
    bound_alias: Option<BindingProvenance>,
    module_alias: Option<BindingProvenance>,
    const_value: Option<BindingProvenance>,
    returned_alias: Option<BindingProvenance>,
    value_alias: Option<NamePath>,
    derived_function_pattern: bool,
) -> DeclarationClassification {
    let name = || match pattern {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        _ => None,
    };
    if let (Some(name), Some(provenance)) = (name(), bound_alias) {
        return DeclarationClassification::Binding { name, provenance };
    }
    if let (Some(name), Some(provenance)) = (name(), module_alias.clone()) {
        return DeclarationClassification::Binding { name, provenance };
    }
    if let Some(BindingProvenance::ModuleNamespace { module }) = module_alias {
        return DeclarationClassification::Require {
            pattern: pattern.clone(),
            module,
        };
    }
    if let Some(init) = init
        && let Some(module) = collector.require_module_expr_name(init)
    {
        return DeclarationClassification::Require {
            pattern: pattern.clone(),
            module,
        };
    }
    if let (Some(name), Some(provenance)) = (name(), const_value) {
        return DeclarationClassification::Binding { name, provenance };
    }
    if value_alias.as_ref().is_none_or(|target| !target.is_root())
        && let (Some(name), Some(provenance)) = (name(), returned_alias)
    {
        return DeclarationClassification::Binding { name, provenance };
    }
    if !derived_function_pattern && let Some(target) = value_alias {
        return DeclarationClassification::ValueAlias {
            pattern: pattern.clone(),
            target,
        };
    }
    DeclarationClassification::None
}

impl Visit for LexicalScopeCollector {
    fn visit_import_decl(&mut self, import: &ImportDecl) {
        let scope = self.current_scope();
        let module = import.src.value.to_string_lossy().to_smolstr();
        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
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
            record_mutable_static_object(self, scope, mutable_object, declarator);
            if register_declared_function(self, scope, declarator) {
                continue;
            }
            if let (Pat::Ident(alias), Some(Expr::Ident(target))) =
                (&declarator.name, declarator.init.as_deref())
                && let Some(function_scope) = self.function_scope_for_name(target.sym.as_ref())
                && let Some(key) = self.scoped_name(scope, alias.id.sym.as_ref())
            {
                self.function_aliases.insert(key, function_scope);
            }
            let init = declarator.init.as_deref();
            let module_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.module_alias_provenance(init));
            let value_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.rooted_name_path(init));
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
            let derived_function_pattern =
                collect_derived_function_pattern(self, &declarator.name, init, scope);

            match classify_declaration(
                self,
                &declarator.name,
                init,
                bound_alias,
                module_alias,
                const_value,
                returned_alias,
                value_alias,
                derived_function_pattern,
            ) {
                DeclarationClassification::Binding { name, provenance } => {
                    self.insert(scope, name, provenance);
                }
                DeclarationClassification::Require { pattern, module } => {
                    self.collect_require_aliases(&pattern, module, scope);
                }
                DeclarationClassification::ValueAlias { pattern, target } => {
                    self.collect_value_aliases(&pattern, &target, scope);
                }
                DeclarationClassification::None => {}
            }
            visit_initializer(self, init);
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let rooted_alias = self.rooted_name_path(&assignment.right);
        let provenance = self
            .bound_callable_provenance(&assignment.right)
            .or_else(|| self.module_alias_provenance(&assignment.right))
            .or_else(|| self.returned_object_provenance(&assignment.right))
            .or_else(|| self.const_provenance(&assignment.right))
            .or_else(|| rooted_alias.map(|target| BindingProvenance::ValueAlias { target }))
            .unwrap_or(BindingProvenance::Local);
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                if let Some((scope, ())) = self.stack.iter().rev().find_map(|scope| {
                    self.name_id(ident.id.sym.as_ref())
                        .is_some_and(|name| self.scopes[*scope].bindings.contains_key(&name))
                        .then_some((ScopeId::from(*scope), ()))
                }) {
                    self.record_assignment(
                        assignment.span,
                        scope,
                        ident.id.sym.as_ref(),
                        provenance,
                    );
                }
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                if let Some(receiver) = self.rooted_name_path(&member.obj) {
                    self.rooted_property_mutations.push(RootedPropertyMutation {
                        span: assignment.span,
                        scope: self.current_scope(),
                        receiver,
                        property: member_property_name(&member.prop)
                            .and_then(|property| self.interned_name(&property)),
                    });
                }
                self.invalidate_member_root(member, assignment.span);
                if let (Some(property), Some(root)) = (
                    member_expression_chain(member),
                    member_root_identifier(member),
                ) {
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
                if let Some(target) = self.rooted_name_path(&assignment.right) {
                    self.collect_assignment_aliases(
                        &pattern,
                        &target,
                        assignment.span,
                        self.current_scope(),
                    );
                }
            }
            AssignTarget::Simple(_) => {}
        }
        assignment.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record_modeled_callbacks(call);
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Ident(callee) = &**callee
        {
            if callee.sym == *"eval" {
                self.dynamic_evals.push((
                    self.binding_scope(VarDeclKind::Var),
                    super::super::ScopeEffect::DynamicEvaluation { span: call.span },
                ));
            }
            if let Ok(callee_name) = self.names.intern(callee.sym.as_ref()) {
                self.calls.push((
                    self.current_scope(),
                    callee_name,
                    call.args
                        .iter()
                        .map(|argument| self.argument_provenance(&argument.expr))
                        .collect(),
                ));
            }
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
        if let Ok(name_id) = self.names.intern(fn_decl.ident.sym.as_ref()) {
            self.function_scopes
                .insert((parent, name_id), (scope, parameters));
        }
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
                self.record_assignment(function.span, scope, name.as_str(), provenance);
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
                self.record_assignment(arrow.span, scope, name.as_str(), provenance);
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

fn collect_derived_function_pattern(
    collector: &mut LexicalScopeCollector,
    pattern: &Pat,
    init: Option<&Expr>,
    scope: ScopeId,
) -> bool {
    let (Pat::Object(object), Some(init)) = (pattern, init) else {
        return false;
    };
    if !function_prototype_builtin(init).is_some_and(|name| collector.is_unbound(name)) {
        return false;
    }
    for property in &object.props {
        if let ObjectPatProp::KeyValue(property) = property
            && property_name(&property.key).as_deref() == Some("constructor")
            && let Some(target) = collector.name_path(&"Function".into())
        {
            collector.collect_value_aliases(&property.value, &target, scope);
        }
    }
    true
}

fn register_declared_function(
    collector: &mut LexicalScopeCollector,
    scope: ScopeId,
    declarator: &VarDeclarator,
) -> bool {
    let (Pat::Ident(ident), Some(init)) = (&declarator.name, declarator.init.as_deref()) else {
        return false;
    };
    let Ok(name_id) = collector.names.intern(ident.id.sym.as_ref()) else {
        return false;
    };
    if !collector.register_function_expression(Some(name_id), init) {
        return false;
    }
    collector.insert_local(scope, ident.id.sym.to_string());
    true
}

fn record_mutable_static_object(
    collector: &mut LexicalScopeCollector,
    scope: ScopeId,
    mutable_object: bool,
    declarator: &VarDeclarator,
) {
    if mutable_object
        && let Pat::Ident(ident) = &declarator.name
        && let Some(name) = collector.scoped_name(scope, ident.id.sym.as_ref())
    {
        collector.mutable_static_objects.insert(name);
    }
}

fn visit_initializer(collector: &mut LexicalScopeCollector, init: Option<&Expr>) {
    if let Some(init) = init {
        init.visit_with(collector);
    }
}
