//! Source-order AST visitor for declarations, assignments, calls, and scopes.
//!
//! The visitor consumes the predeclared scope tree and records only
//! use-position facts that survive lexical shadowing, reassignment, and
//! unsupported dynamic forms.

use hashbrown::HashMap;
use smol_str::SmolStr;
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, CallExpr, Callee, Expr, FnDecl, ObjectPatProp, Pat,
    SimpleAssignTarget, VarDecl, VarDeclKind,
};

use crate::analysis::{
    scope::{
        BindingProvenance, ScopeCollector,
        ScopeEffect::DynamicEvaluation,
        ScopeId, ScopeKind, ScopedName,
        build::{
            CompactPat, ControlFlowFrame, PropertyAliasAssignment, RootedPropertyMutation,
            ScopeCollectionIssue, ScopedDynamicEval,
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
        let Some(parent) = self.current_scope() else {
            self.artifacts
                .record_issue(ScopeCollectionIssue::ScopeStackUnderflow);
            return ScopeEntry::Rejected;
        };
        if let Some(scope_id) = self
            .lexical
            .scope_shapes
            .take_child(Some(parent), span.lo, kind)
        {
            self.lexical.stack.push(scope_id);
            #[cfg(test)]
            {
                self.scope_lookups += 1;
            }
            ScopeEntry::Entered(scope_id)
        } else {
            self.artifacts
                .record_issue(ScopeCollectionIssue::ShapeMismatch);
            ScopeEntry::Rejected
        }
    }

    fn pop_scope(&mut self, entry: ScopeEntry) {
        if matches!(entry, ScopeEntry::Entered(_)) {
            if self.lexical.stack.len() <= 1 {
                self.artifacts
                    .record_issue(ScopeCollectionIssue::ScopeStackUnderflow);
                return;
            }
            let _ = self.lexical.stack.pop();
        }
    }

    fn current_scope(&self) -> Option<ScopeId> {
        if self.artifacts.has_issues() {
            None
        } else {
            self.lexical.stack.last().copied()
        }
    }

    fn is_budget_exhausted(&self) -> bool {
        self.budget.exhausted()
    }

    fn enter_if(&mut self) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::If {
                incoming,
                consequent: None,
            });
        self.assignment.path.assignment_writes.clear();
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    fn enter_else(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::If {
                incoming,
                consequent,
            }) = self.assignment.path.control_flow.last_mut()
            else {
                return;
            };
            *consequent = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    fn exit_if(&mut self, span: swc_common::Span, has_else: bool) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::If {
            incoming,
            consequent,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let consequent = consequent.unwrap_or_else(|| self.checkpoint());
        let paths = if has_else {
            vec![consequent, self.checkpoint()]
        } else {
            vec![incoming.clone(), consequent]
        };
        self.join_paths(span, &incoming, &paths);
    }

    fn enter_loop(&mut self, guaranteed: bool) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Loop {
                incoming,
                guaranteed,
                breaks: Vec::new(),
                continues: Vec::new(),
            });
        self.assignment.path.assignment_writes.clear();
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    fn exit_loop(&mut self, span: swc_common::Span) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Loop {
            incoming,
            guaranteed,
            breaks,
            continues,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let body = self.checkpoint();
        let mut paths = Vec::with_capacity(breaks.len() + 2);
        if !guaranteed {
            paths.push(incoming.clone());
        }
        paths.push(body);
        paths.extend(breaks);
        paths.extend(continues);
        self.join_paths(span, &incoming, &paths);
    }

    fn enter_switch(&mut self) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Switch {
                incoming,
                cases: Vec::new(),
                breaks: Vec::new(),
            });
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_add(1);
    }

    fn enter_switch_case(&mut self) {
        let incoming = {
            let Some(ControlFlowFrame::Switch { incoming, .. }) =
                self.assignment.path.control_flow.last()
            else {
                return;
            };
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    fn exit_switch_case(&mut self) {
        let case = self.checkpoint();
        if let Some(ControlFlowFrame::Switch { cases, .. }) =
            self.assignment.path.control_flow.last_mut()
        {
            cases.push(case);
        }
    }

    fn exit_switch(&mut self, span: swc_common::Span) {
        self.assignment.path.conditional_depth =
            self.assignment.path.conditional_depth.saturating_sub(1);
        let Some(ControlFlowFrame::Switch {
            incoming,
            cases,
            breaks,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        let mut paths = Vec::with_capacity(cases.len() + breaks.len() + 1);
        paths.push(incoming.clone());
        paths.extend(cases);
        paths.extend(breaks);
        self.join_paths(span, &incoming, &paths);
    }

    fn enter_try(&mut self, has_handler: bool, has_finally: bool) {
        let incoming = self.checkpoint();
        self.assignment
            .path
            .control_flow
            .push(ControlFlowFrame::Try {
                incoming,
                body: None,
                conditional: has_handler || has_finally,
            });
        self.assignment.path.assignment_writes.clear();
        if has_handler || has_finally {
            self.assignment.path.conditional_depth =
                self.assignment.path.conditional_depth.saturating_add(1);
        }
    }

    fn enter_catch(&mut self) {
        let checkpoint = self.checkpoint();
        let incoming = {
            let Some(ControlFlowFrame::Try { incoming, body, .. }) =
                self.assignment.path.control_flow.last_mut()
            else {
                return;
            };
            *body = Some(checkpoint);
            incoming.clone()
        };
        self.restore(&incoming);
        self.assignment.path.assignment_writes.clear();
    }

    fn exit_try(&mut self, span: swc_common::Span, has_handler: bool, has_finally: bool) {
        let Some(ControlFlowFrame::Try {
            incoming,
            body,
            conditional,
        }) = self.assignment.path.control_flow.pop()
        else {
            return;
        };
        if conditional {
            self.assignment.path.conditional_depth =
                self.assignment.path.conditional_depth.saturating_sub(1);
        }
        let body = body.unwrap_or_else(|| self.checkpoint());
        let mut paths = Vec::new();
        if has_handler {
            paths.push(body);
            paths.push(self.checkpoint());
        } else if has_finally {
            paths.push(incoming.clone());
            paths.push(body);
        } else {
            paths.push(body);
        }
        self.join_paths(span, &incoming, &paths);
    }

    fn mark_unreachable(&mut self) {
        self.assignment.path.reachable = false;
    }

    fn break_exit(&mut self) {
        if self.assignment.path.reachable {
            let checkpoint = self.checkpoint();
            if let Some(frame) = self
                .assignment
                .path
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| {
                    matches!(
                        frame,
                        ControlFlowFrame::Loop { .. } | ControlFlowFrame::Switch { .. }
                    )
                })
            {
                match frame {
                    ControlFlowFrame::Loop { breaks, .. }
                    | ControlFlowFrame::Switch { breaks, .. } => breaks.push(checkpoint),
                    _ => unreachable!("breakable frame was checked above"),
                }
            }
        }
        self.assignment.path.reachable = false;
    }

    fn continue_exit(&mut self) {
        if self.assignment.path.reachable {
            let checkpoint = self.checkpoint();
            if let Some(ControlFlowFrame::Loop { continues, .. }) = self
                .assignment
                .path
                .control_flow
                .iter_mut()
                .rev()
                .find(|frame| matches!(frame, ControlFlowFrame::Loop { .. }))
            {
                continues.push(checkpoint);
            }
        }
        self.assignment.path.reachable = false;
    }

    fn enter_function(&mut self) {
        let checkpoint = self.checkpoint();
        let control_depth = self.assignment.path.control_flow.len();
        self.assignment
            .path
            .function_checkpoints
            .push(super::FunctionCheckpoint {
                checkpoint,
                conditional_depth: self.assignment.path.conditional_depth,
                control_depth,
            });
        self.assignment.path.reachable = true;
        self.assignment.path.assignment_writes.clear();
    }

    fn exit_function(&mut self) {
        let Some(super::FunctionCheckpoint {
            checkpoint,
            conditional_depth,
            control_depth,
        }) = self.assignment.path.function_checkpoints.pop()
        else {
            return;
        };
        self.assignment.path.control_flow.truncate(control_depth);
        self.assignment.path.conditional_depth = conditional_depth;
        self.restore(&checkpoint);
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
            self.install_function_binding(scope, &pending, parameters);
        }
        if let Some(bindings) = self.functions.inline_parameters.remove(&function.span.lo) {
            self.install_inline_parameters(function.span, scope, bindings);
        }
    }

    fn after_arrow(&mut self, arrow: &ArrowExpr, scope: ScopeId) {
        if let Some(pending) = self.functions.pending_function_names.remove(&arrow.span.lo) {
            let parameters = Self::arrow_parameters(arrow);
            self.install_function_binding(scope, &pending, parameters);
        }
        if let Some(bindings) = self.functions.inline_parameters.remove(&arrow.span.lo) {
            self.install_inline_parameters(arrow.span, scope, bindings);
        }
    }
}

impl ScopeCollector<'_> {
    fn install_function_binding(
        &mut self,
        scope: ScopeId,
        pending: &super::PendingFunctionName,
        parameters: Vec<CompactPat>,
    ) {
        self.functions.function_scopes.insert(
            ScopedName::new(pending.declaration_scope, pending.name),
            super::FunctionBinding { scope, parameters },
        );
    }

    fn install_inline_parameters(
        &mut self,
        span: swc_common::Span,
        scope: ScopeId,
        bindings: HashMap<SmolStr, BindingProvenance>,
    ) {
        for (name, provenance) in bindings {
            self.record_assignment(span, scope, name.as_str(), provenance);
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
                        .and_then(|property| self.name_id(&property)),
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
        if let Some(target) = self.rooted_name_path(&assignment.right) {
            self.collect_assignment_aliases(pattern, &target, assignment.span, scope);
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
