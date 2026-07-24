//! Source-order AST visitor for declarations, assignments, calls, and scopes.
//!
//! The visitor consumes the predeclared scope tree and records only
//! use-position facts that survive lexical shadowing, reassignment, and
//! unsupported dynamic forms.

use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, CallExpr, Callee, ClassDecl, Expr, FnDecl, Function,
    ImportDecl, ObjectPatProp, Pat, SimpleAssignTarget, VarDecl, VarDeclKind,
};

use super::traversal::ScopePass;
use crate::analysis::{
    scope::{
        ScopeCollector,
        ScopeEffect::DynamicEvaluation,
        ScopeId, ScopeKind,
        collect::{
            PropertyAliasAssignment, RootedPropertyMutation,
            analysis::{
                DeclarationClassification, assignment_provenance, classify_declaration,
                expression_is_mutable_static_object,
            },
        },
    },
    syntax::{
        function_prototype_builtin, member_expression_chain, member_property_name,
        member_root_identifier, property_name,
    },
};

impl ScopePass for ScopeCollector<'_> {
    fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) {
        self.push_scope(span, kind);
    }

    fn pop_scope(&mut self) {
        self.pop_scope();
    }

    fn current_scope(&self) -> ScopeId {
        self.current_scope()
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.insert_import(self.current_scope(), import);
    }

    fn visit_var_decl(&mut self, var_decl: &VarDecl) {
        let scope = self.binding_scope(var_decl.kind);
        for declarator in &var_decl.decls {
            let init = declarator.init.as_deref();
            let mutable_object = init
                .is_some_and(|init| expression_is_mutable_static_object(self, init, var_decl.kind));
            record_mutable_static_object(self, scope, mutable_object, declarator);

            // Stash pending function expression names so after_arrow /
            // after_function hooks can record function_scopes metadata.
            if let (Pat::Ident(ident), Some(init)) = (&declarator.name, init) {
                self.budget.try_charge();
                if let Ok(name_id) = self.names.intern(ident.id.sym.as_ref()) {
                    if let Expr::Arrow(arrow) = init {
                        self.pending_function_names
                            .insert(arrow.span.lo, (scope, name_id));
                    } else if let Expr::Fn(func_expr) = init {
                        self.pending_function_names
                            .insert(func_expr.function.span.lo, (scope, name_id));
                    }
                } else {
                    self.name_exhausted = true;
                }
            }

            if let (Pat::Ident(alias), Some(Expr::Ident(target))) = (&declarator.name, init)
                && let Some(function_scope) = self.function_scope_for_name(target.sym.as_ref())
                && let Some(key) = self.scoped_name(scope, alias.id.sym.as_ref())
            {
                self.function_aliases.insert(key, function_scope);
            }
            self.insert_pat_locals(scope, &declarator.name);
            let derived_function_pattern =
                collect_derived_function_pattern(self, &declarator.name, init, scope);

            if let Some(init) = init {
                match classify_declaration(self, init, &declarator.name, derived_function_pattern) {
                    DeclarationClassification::Binding { name, provenance } => {
                        self.insert(scope, name, provenance);
                    }
                    DeclarationClassification::Require { module } => {
                        self.collect_require_aliases(&declarator.name, module, scope);
                    }
                    DeclarationClassification::ValueAlias { target } => {
                        self.collect_value_aliases(&declarator.name, &target, scope);
                    }
                    DeclarationClassification::None => {}
                }
            }
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                let provenance = assignment_provenance(self, &assignment.right);
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
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record_modeled_callbacks(call);
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Ident(callee) = &**callee
        {
            if callee.sym == *"eval" {
                self.dynamic_evals.push((
                    self.binding_scope(VarDeclKind::Var),
                    DynamicEvaluation { span: call.span },
                ));
            }
            self.budget.try_charge();
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
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl) {
        self.insert_local(self.current_scope(), class_decl.ident.sym.to_string());
    }

    fn visit_catch_param(&mut self, pat: &Pat) {
        self.insert_pat_locals(self.current_scope(), pat);
    }

    fn before_fn_decl(&mut self, fn_decl: &FnDecl, parent: ScopeId) {
        self.insert_local(parent, fn_decl.ident.sym.to_string());
    }

    fn after_fn_decl(&mut self, fn_decl: &FnDecl, scope: ScopeId) {
        let parameters = Self::function_parameters(&fn_decl.function);
        self.budget.try_charge();
        if let Ok(name_id) = self.names.intern(fn_decl.ident.sym.as_ref()) {
            let parent = self
                .scopes
                .get(scope.index())
                .and_then(|s| s.parent)
                .unwrap_or_else(|| ScopeId::from(0));
            self.function_scopes
                .insert((parent, name_id), (scope, parameters));
        }
        for param in &fn_decl.function.params {
            self.insert_pat_locals(scope, &param.pat);
        }
    }

    fn after_function(&mut self, function: &Function, scope: ScopeId) {
        for param in &function.params {
            self.insert_pat_locals(scope, &param.pat);
        }
        if let Some((decl_scope, name_id)) = self.pending_function_names.remove(&function.span.lo) {
            let parameters = Self::function_parameters(function);
            self.function_scopes
                .insert((decl_scope, name_id), (scope, parameters));
        }
        if let Some(bindings) = self.inline_parameters.get(&function.span.lo).cloned() {
            for (name, provenance) in bindings {
                self.record_assignment(function.span, scope, name.as_str(), provenance);
            }
        }
    }

    fn after_arrow(&mut self, arrow: &ArrowExpr, scope: ScopeId) {
        for param in &arrow.params {
            self.insert_pat_locals(scope, param);
        }
        if let Some((decl_scope, name_id)) = self.pending_function_names.remove(&arrow.span.lo) {
            let parameters = Self::arrow_parameters(arrow);
            self.function_scopes
                .insert((decl_scope, name_id), (scope, parameters));
        }
        if let Some(bindings) = self.inline_parameters.get(&arrow.span.lo).cloned() {
            for (name, provenance) in bindings {
                self.record_assignment(arrow.span, scope, name.as_str(), provenance);
            }
        }
    }
}

fn collect_derived_function_pattern(
    collector: &mut ScopeCollector,
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

fn record_mutable_static_object(
    collector: &mut ScopeCollector,
    scope: ScopeId,
    mutable_object: bool,
    declarator: &swc_ecma_ast::VarDeclarator,
) {
    if mutable_object
        && let Pat::Ident(ident) = &declarator.name
        && let Some(name) = collector.scoped_name(scope, ident.id.sym.as_ref())
    {
        collector.mutable_static_objects.insert(name);
    }
}
