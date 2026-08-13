//! Source-order AST visitor for declarations, assignments, calls, and scopes.
//!
//! The visitor consumes the predeclared scope tree and records only
//! use-position facts that survive lexical shadowing, reassignment, and
//! unsupported dynamic forms.

use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, CallExpr, Callee, Expr, FnDecl, ObjectPatProp, Pat,
    SimpleAssignTarget, VarDecl, VarDeclKind,
};

use crate::analysis::{
    scope::{
        ScopeCollector,
        ScopeEffect::DynamicEvaluation,
        ScopeId, ScopeKind, ScopedName,
        build::{
            PropertyAliasAssignment, RootedPropertyMutation, ScopedDynamicEval,
            analysis::{
                DeclarationClassification, assignment_provenance, classify_declaration,
                expression_is_mutable_static_object,
            },
            traversal::{ScopeEntry, ScopePass},
        },
    },
    syntax::{
        function_prototype_builtin, literal_member_property_name, literal_property_name,
        member_expression_chain, member_root_identifier,
    },
};

impl ScopePass for ScopeCollector<'_> {
    fn push_scope(&mut self, span: swc_common::Span, kind: ScopeKind) -> ScopeEntry {
        self.push_scope(span, kind)
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

    fn enter_function(&mut self) {
        self.enter_function();
    }

    fn exit_function(&mut self) {
        self.exit_function();
    }

    fn enter_if(&mut self) {
        self.enter_if();
    }

    fn enter_else(&mut self) {
        self.enter_else();
    }

    fn exit_if(&mut self, span: swc_common::Span, has_else: bool) {
        self.exit_if(span, has_else);
    }

    fn enter_loop(&mut self, guaranteed: bool) {
        self.enter_loop(guaranteed);
    }

    fn exit_loop(&mut self, span: swc_common::Span) {
        self.exit_loop(span);
    }

    fn enter_switch(&mut self) {
        self.enter_switch();
    }

    fn enter_switch_case(&mut self) {
        self.enter_switch_case();
    }

    fn exit_switch_case(&mut self) {
        self.exit_switch_case();
    }

    fn exit_switch(&mut self, span: swc_common::Span) {
        self.exit_switch(span);
    }

    fn enter_try(&mut self, has_handler: bool, has_finally: bool) {
        self.enter_try(has_handler, has_finally);
    }

    fn enter_catch(&mut self) {
        self.enter_catch();
    }

    fn exit_try(&mut self, span: swc_common::Span, has_handler: bool, has_finally: bool) {
        self.exit_try(span, has_handler, has_finally);
    }

    fn mark_unreachable(&mut self) {
        self.mark_unreachable();
    }

    fn break_exit(&mut self) {
        self.break_exit();
    }

    fn continue_exit(&mut self) {
        self.continue_exit();
    }

    fn visit_var_decl(&mut self, var_decl: &VarDecl) {
        let Some(scope) = self.binding_scope(var_decl.kind) else {
            return;
        };
        for declarator in &var_decl.decls {
            let init = declarator.init.as_deref();
            self.record_declaration_metadata(scope, var_decl.kind, declarator, init);
            self.reset_pat_locals(scope, &declarator.name);
            let derived_function_pattern =
                collect_derived_function_pattern(self, &declarator.name, init, scope);
            self.record_declaration_provenance(
                scope,
                &declarator.name,
                init,
                derived_function_pattern,
            );
        }
    }

    fn visit_assign_expr(&mut self, assignment: &AssignExpr) {
        let Some(scope) = self.current_scope() else {
            return;
        };
        match &assignment.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(ident)) => {
                self.record_identifier_assignment(ident, assignment);
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                self.record_member_assignment(scope, member, assignment);
            }
            AssignTarget::Pat(pattern) => {
                self.record_destructuring_assignment(scope, pattern, assignment);
            }
            AssignTarget::Simple(_) => {}
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.record_modeled_callbacks(call);
        if let Callee::Expr(callee) = &call.callee
            && let Expr::Ident(callee) = &**callee
        {
            self.record_dynamic_eval(call, callee);
            self.record_function_call(call, callee);
        }
    }

    fn visit_catch_param(&mut self, pat: &Pat) {
        if let Some(scope) = self.current_scope() {
            self.register_pat_locals(scope, pat);
        }
    }

    fn after_fn_decl(&mut self, fn_decl: &FnDecl, scope: ScopeId) {
        let parameters = Self::function_parameters(&fn_decl.function);
        self.budget.try_charge();
        if let Some(name_id) = self.lookup_or_intern_name(fn_decl.ident.sym.as_ref()) {
            let parent = self
                .lexical
                .scopes
                .get(scope)
                .and_then(crate::analysis::model::scope::LexicalScope::parent)
                .unwrap_or_default();
            self.functions.function_scopes.insert(
                ScopedName::new(parent, name_id),
                super::FunctionBinding { scope, parameters },
            );
        }
    }

    fn after_function(&mut self, function: &swc_ecma_ast::Function, scope: ScopeId) {
        if let Some(pending) = self
            .functions
            .pending_function_names
            .remove(&function.span.lo)
        {
            let parameters = Self::function_parameters(function);
            self.functions.function_scopes.insert(
                ScopedName::new(pending.declaration_scope, pending.name),
                super::FunctionBinding { scope, parameters },
            );
        }
        if let Some(bindings) = self.functions.inline_parameters.remove(&function.span.lo) {
            for (name, provenance) in bindings {
                self.record_assignment(function.span, scope, name.as_str(), provenance);
            }
        }
    }

    fn after_arrow(&mut self, arrow: &ArrowExpr, scope: ScopeId) {
        if let Some(pending) = self.functions.pending_function_names.remove(&arrow.span.lo) {
            let parameters = Self::arrow_parameters(arrow);
            self.functions.function_scopes.insert(
                ScopedName::new(pending.declaration_scope, pending.name),
                super::FunctionBinding { scope, parameters },
            );
        }
        if let Some(bindings) = self.functions.inline_parameters.remove(&arrow.span.lo) {
            for (name, provenance) in bindings {
                self.record_assignment(arrow.span, scope, name.as_str(), provenance);
            }
        }
    }
}

impl ScopeCollector<'_> {
    fn record_declaration_metadata(
        &mut self,
        scope: ScopeId,
        kind: VarDeclKind,
        declarator: &swc_ecma_ast::VarDeclarator,
        init: Option<&Expr>,
    ) {
        let mutable_object =
            init.is_some_and(|init| expression_is_mutable_static_object(self, init, kind));
        record_mutable_static_object(self, scope, mutable_object, declarator);
        self.record_pending_function_name(scope, &declarator.name, init);
        self.record_function_alias(scope, &declarator.name, init);
    }

    fn record_pending_function_name(&mut self, scope: ScopeId, pattern: &Pat, init: Option<&Expr>) {
        let (Pat::Ident(ident), Some(init)) = (pattern, init) else {
            return;
        };
        self.budget.try_charge();
        let Some(name_id) = self.lookup_or_intern_name(ident.id.sym.as_ref()) else {
            self.lexical.name_exhausted = true;
            return;
        };
        let span = match init {
            Expr::Arrow(arrow) => arrow.span,
            Expr::Fn(function) => function.function.span,
            _ => return,
        };
        self.functions.pending_function_names.insert(
            span.lo,
            super::PendingFunctionName {
                declaration_scope: scope,
                name: name_id,
            },
        );
    }

    fn record_function_alias(&mut self, scope: ScopeId, pattern: &Pat, init: Option<&Expr>) {
        let (Pat::Ident(alias), Some(Expr::Ident(target))) = (pattern, init) else {
            return;
        };
        let Some(function_scope) = self.function_scope_for_name(target.sym.as_ref()) else {
            return;
        };
        if let Some(key) = self.scoped_name(scope, alias.id.sym.as_ref()) {
            self.functions.function_aliases.insert(key, function_scope);
        }
    }

    fn record_declaration_provenance(
        &mut self,
        scope: ScopeId,
        pattern: &Pat,
        init: Option<&Expr>,
        derived_function_pattern: bool,
    ) {
        let Some(init) = init else {
            return;
        };
        match classify_declaration(self, init, pattern, derived_function_pattern) {
            DeclarationClassification::Binding { name, provenance } => {
                self.update_binding(scope, name, provenance);
            }
            DeclarationClassification::Require { module } => {
                self.collect_require_aliases(pattern, module, scope);
            }
            DeclarationClassification::ValueAlias { target } => {
                self.collect_value_aliases(pattern, &target, scope);
            }
            DeclarationClassification::None => {}
        }
    }

    fn record_identifier_assignment(
        &mut self,
        ident: &swc_ecma_ast::BindingIdent,
        assignment: &AssignExpr,
    ) {
        let provenance = assignment_provenance(self, &assignment.right);
        let Some(name_id) = self.name_id(ident.id.sym.as_ref()) else {
            return;
        };
        if let Some(binding_scope) = self.lexical.stack.iter().rev().find_map(|scope| {
            self.lexical
                .scopes
                .get(*scope)
                .is_some_and(|scope| scope.has_binding(name_id))
                .then_some(*scope)
        }) {
            self.record_assignment(
                assignment.span,
                binding_scope,
                ident.id.sym.as_ref(),
                provenance,
            );
        }
    }

    fn record_member_assignment(
        &mut self,
        scope: ScopeId,
        member: &swc_ecma_ast::MemberExpr,
        assignment: &AssignExpr,
    ) {
        if let Some(receiver) = self.rooted_name_path(&member.obj) {
            self.artifacts
                .record_rooted_property_mutation(RootedPropertyMutation::new(
                    assignment.span,
                    scope,
                    receiver,
                    literal_member_property_name(&member.prop)
                        .and_then(|property| self.interned_name(&property)),
                ));
        }
        self.invalidate_member_root(member, assignment.span);
        if let (Some(property), Some(root)) = (
            member_expression_chain(member),
            member_root_identifier(member),
        ) {
            self.artifacts
                .record_property_assignment(PropertyAliasAssignment::new(
                    assignment.span,
                    scope,
                    property,
                    root.clone(),
                    self.rooted_expr_name(&assignment.right),
                ));
        }
    }

    fn record_destructuring_assignment(
        &mut self,
        scope: ScopeId,
        pattern: &swc_ecma_ast::AssignTargetPat,
        assignment: &AssignExpr,
    ) {
        let pattern: Pat = pattern.clone().into();
        if let Some(target) = self.rooted_name_path(&assignment.right) {
            self.collect_assignment_aliases(&pattern, &target, assignment.span, scope);
        }
    }

    fn record_dynamic_eval(&mut self, call: &CallExpr, callee: &swc_ecma_ast::Ident) {
        if callee.sym == *"eval"
            && let Some(scope) = self.binding_scope(VarDeclKind::Var)
        {
            self.artifacts.record_dynamic_eval(ScopedDynamicEval::new(
                scope,
                DynamicEvaluation { span: call.span },
            ));
        }
    }

    fn record_function_call(&mut self, call: &CallExpr, callee: &swc_ecma_ast::Ident) {
        self.budget.try_charge();
        let Some(callee_name) = self.lookup_or_intern_name(callee.sym.as_ref()) else {
            return;
        };
        let arguments = call
            .args
            .iter()
            .map(|argument| self.argument_provenance(&argument.expr))
            .collect();
        if let Some(caller_scope) = self.current_scope() {
            self.functions.calls.push(super::FunctionCall {
                caller_scope,
                callee_name,
                arguments,
            });
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
            && literal_property_name(&property.key).as_deref() == Some("constructor")
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
        collector.artifacts.record_mutable_static_object(name);
    }
}
