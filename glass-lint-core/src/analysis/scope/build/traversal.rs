use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BlockStmt, BreakStmt, CallExpr, CatchClause, ClassDecl, ContinueStmt,
    DoWhileStmt, FnDecl, ForInStmt, ForOfStmt, ForStmt, Function, Ident, IfStmt, ImportDecl, Lit,
    MemberExpr, Pat, PropName, ReturnStmt, SwitchCase, SwitchStmt, ThrowStmt, TryStmt, VarDecl,
    WhileStmt, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use crate::analysis::scope::{ScopeId, ScopeKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis::scope) enum ScopeEntry {
    Entered(ScopeId),
    Rejected,
}

/// Phase-specific policy for scope traversal.
///
/// Each phase owns its own scope stack, `push_scope`/`pop_scope` logic, and
/// non-scope-forming visit overrides. Default trait methods are no-ops so a
/// pass overrides only the methods it needs.
pub(in crate::analysis::scope) trait ScopePass {
    /// Enter one predeclared scope. `false` means the planned shape was not
    /// available, so the traversal must skip that subtree and keep the stack
    /// balanced without inventing a fallback scope.
    fn push_scope(&mut self, span: Span, kind: ScopeKind) -> ScopeEntry;
    fn pop_scope(&mut self, entry: ScopeEntry);
    fn current_scope(&self) -> Option<ScopeId>;

    /// Returns `true` when the semantic budget is exhausted.
    /// The traversal skips child descent after exhaustion so the AST walk
    /// terminates quickly instead of visiting every remaining node.
    fn is_budget_exhausted(&self) -> bool {
        false
    }

    // === SCOPE-FORMING HOOKS ===
    // Called by the traversal at specific points. MUST NOT call visit_with.

    /// Before entering a function declaration scope. Declare the name in the
    /// parent scope here.
    fn before_fn_decl(&mut self, _decl: &FnDecl, _parent: ScopeId) {}
    /// After entering a function declaration scope, before visiting children.
    fn after_fn_decl(&mut self, _decl: &FnDecl, _scope: ScopeId) {}
    /// After entering a function expression scope, before visiting children.
    fn after_function(&mut self, _func: &Function, _scope: ScopeId) {}
    /// After entering an arrow expression scope, before visiting children.
    fn after_arrow(&mut self, _arrow: &ArrowExpr, _scope: ScopeId) {}
    fn enter_function(&mut self) {}
    fn exit_function(&mut self) {}

    // === NON-SCOPE-FORMING VISIT HOOKS ===
    // Called by the traversal before visiting children. Default is no-op.
    // The traversal visits children after each hook.

    fn visit_ident(&mut self, _ident: &Ident) {}
    fn visit_member_expr(&mut self, _member: &MemberExpr) {}
    fn visit_prop_name(&mut self, _prop: &PropName) {}
    fn visit_lit(&mut self, _lit: &Lit) {}
    fn visit_import_decl(&mut self, _import: &ImportDecl) {}
    fn visit_var_decl(&mut self, _decl: &VarDecl) {}
    fn visit_assign_expr(&mut self, _expr: &AssignExpr) {}
    fn visit_call_expr(&mut self, _call: &CallExpr) {}
    fn visit_class_decl(&mut self, _decl: &ClassDecl) {}
    /// Called when entering a catch clause parameter pattern.
    /// The pass should register the parameter bindings in the current scope.
    fn visit_catch_param(&mut self, _pat: &Pat) {}
    fn enter_if(&mut self) {}
    fn enter_else(&mut self) {}
    fn exit_if(&mut self, _span: Span, _has_else: bool) {}
    fn enter_loop(&mut self, _guaranteed: bool) {}
    fn exit_loop(&mut self, _span: Span) {}
    fn enter_switch(&mut self) {}
    fn enter_switch_case(&mut self) {}
    fn exit_switch_case(&mut self) {}
    fn exit_switch(&mut self, _span: Span) {}
    fn enter_try(&mut self, _has_handler: bool, _has_finally: bool) {}
    fn enter_catch(&mut self) {}
    fn exit_try(&mut self, _span: Span, _has_handler: bool, _has_finally: bool) {}
    fn break_exit(&mut self) {}
    fn continue_exit(&mut self) {}
    fn mark_unreachable(&mut self) {}
}

/// Phase-neutral scope traversal.
///
/// Owns the `Visit` methods for all scope-forming syntax and delegates
/// phase-specific work to the generic `P: ScopePass`.
pub(in crate::analysis::scope) struct ScopeTraversal<P> {
    pub(super) pass: P,
}

impl<P: ScopePass> ScopeTraversal<P> {
    pub(in crate::analysis::scope) fn new(pass: P) -> Self {
        Self { pass }
    }

    pub(in crate::analysis::scope) fn into_pass(self) -> P {
        self.pass
    }

    fn visit_scoped_body(
        &mut self,
        span: Span,
        kind: ScopeKind,
        before_body: impl FnOnce(&mut P, ScopeId),
        body: impl FnOnce(&mut Self),
    ) {
        let entry = self.pass.push_scope(span, kind);
        if let ScopeEntry::Entered(scope) = entry {
            before_body(&mut self.pass, scope);
            if !self.pass.is_budget_exhausted() {
                body(self);
            }
        }
        self.pass.pop_scope(entry);
    }

    fn visit_function_body(
        &mut self,
        span: Span,
        before_body: impl FnOnce(&mut P, ScopeId),
        body: impl FnOnce(&mut Self),
    ) {
        let entry = self.pass.push_scope(span, ScopeKind::Function);
        if let ScopeEntry::Entered(scope) = entry {
            self.pass.enter_function();
            before_body(&mut self.pass, scope);
            if !self.pass.is_budget_exhausted() {
                body(self);
            }
            self.pass.exit_function();
        }
        self.pass.pop_scope(entry);
    }

    fn visit_loop_body(&mut self, guaranteed: bool, span: Span, body: impl FnOnce(&mut Self)) {
        self.pass.enter_loop(guaranteed);
        if !self.pass.is_budget_exhausted() {
            body(self);
        }
        self.pass.exit_loop(span);
    }

    fn visit_scoped_loop(
        &mut self,
        span: Span,
        header: impl FnOnce(&mut Self),
        body: impl FnOnce(&mut Self),
    ) {
        let entry = self.pass.push_scope(span, ScopeKind::Block);
        if matches!(entry, ScopeEntry::Entered(_)) {
            header(self);
            self.visit_loop_body(false, span, body);
        }
        self.pass.pop_scope(entry);
    }
}

impl<P: ScopePass> Visit for ScopeTraversal<P> {
    fn visit_ident(&mut self, ident: &Ident) {
        self.pass.visit_ident(ident);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        self.pass.visit_member_expr(member);
        if !self.pass.is_budget_exhausted() {
            member.visit_children_with(self);
        }
    }

    fn visit_prop_name(&mut self, prop: &PropName) {
        self.pass.visit_prop_name(prop);
        if !self.pass.is_budget_exhausted() {
            prop.visit_children_with(self);
        }
    }

    fn visit_lit(&mut self, lit: &Lit) {
        self.pass.visit_lit(lit);
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.pass.visit_import_decl(import);
        if !self.pass.is_budget_exhausted() {
            import.visit_children_with(self);
        }
    }

    fn visit_var_decl(&mut self, decl: &VarDecl) {
        self.pass.visit_var_decl(decl);
        if !self.pass.is_budget_exhausted() {
            decl.visit_children_with(self);
        }
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr) {
        self.pass.visit_assign_expr(expr);
        if !self.pass.is_budget_exhausted() {
            expr.visit_children_with(self);
        }
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.pass.visit_call_expr(call);
        if !self.pass.is_budget_exhausted() {
            call.visit_children_with(self);
        }
    }

    fn visit_class_decl(&mut self, decl: &ClassDecl) {
        self.pass.visit_class_decl(decl);
        if !self.pass.is_budget_exhausted() {
            decl.visit_children_with(self);
        }
    }

    // === SCOPE-FORMING METHODS ===

    fn visit_fn_decl(&mut self, decl: &FnDecl) {
        let Some(parent) = self.pass.current_scope() else {
            return;
        };
        self.pass.before_fn_decl(decl, parent);
        self.visit_function_body(
            decl.function.span,
            |pass, scope| pass.after_fn_decl(decl, scope),
            |traversal| {
                for param in &decl.function.params {
                    param.pat.visit_with(traversal);
                }
                decl.function.decorators.visit_with(traversal);
                decl.function.body.visit_with(traversal);
            },
        );
    }

    fn visit_function(&mut self, func: &Function) {
        self.visit_function_body(
            func.span,
            |pass, scope| pass.after_function(func, scope),
            |traversal| {
                for param in &func.params {
                    param.pat.visit_with(traversal);
                }
                func.decorators.visit_with(traversal);
                func.body.visit_with(traversal);
            },
        );
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.visit_function_body(
            arrow.span,
            |pass, scope| pass.after_arrow(arrow, scope),
            |traversal| {
                for param in &arrow.params {
                    param.visit_with(traversal);
                }
                arrow.body.visit_with(traversal);
            },
        );
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.visit_scoped_body(
            block.span,
            ScopeKind::Block,
            |_, _| {},
            |traversal| block.stmts.visit_with(traversal),
        );
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        stmt.discriminant.visit_with(self);
        let entry = self.pass.push_scope(stmt.span, ScopeKind::Block);
        if matches!(entry, ScopeEntry::Entered(_)) {
            self.pass.enter_switch();
            if !self.pass.is_budget_exhausted() {
                for case in &stmt.cases {
                    self.visit_switch_case(case);
                }
            }
            self.pass.exit_switch(stmt.span);
        }
        self.pass.pop_scope(entry);
    }

    fn visit_switch_case(&mut self, case: &SwitchCase) {
        self.pass.enter_switch_case();
        if !self.pass.is_budget_exhausted() {
            case.test.visit_with(self);
            case.cons.visit_with(self);
        }
        self.pass.exit_switch_case();
    }

    fn visit_with_stmt(&mut self, stmt: &WithStmt) {
        stmt.obj.visit_with(self);
        let entry = self.pass.push_scope(stmt.body.span(), ScopeKind::Dynamic);
        if matches!(entry, ScopeEntry::Entered(_)) && !self.pass.is_budget_exhausted() {
            stmt.body.visit_with(self);
        }
        self.pass.pop_scope(entry);
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause) {
        let entry = self.pass.push_scope(clause.span, ScopeKind::Block);
        if matches!(entry, ScopeEntry::Entered(_)) {
            if let Some(param) = &clause.param {
                self.pass.visit_catch_param(param);
            }
            if !self.pass.is_budget_exhausted() {
                clause.body.stmts.visit_with(self);
            }
        }
        self.pass.pop_scope(entry);
    }

    fn visit_try_stmt(&mut self, stmt: &TryStmt) {
        let has_handler = stmt.handler.is_some();
        let has_finally = stmt.finalizer.is_some();
        self.pass.enter_try(has_handler, has_finally);
        if self.pass.is_budget_exhausted() {
            self.pass.exit_try(stmt.span, has_handler, has_finally);
        } else {
            stmt.block.visit_with(self);
            if let Some(handler) = &stmt.handler {
                self.pass.enter_catch();
                handler.visit_with(self);
            }
            self.pass.exit_try(stmt.span, has_handler, has_finally);
            if let Some(finalizer) = &stmt.finalizer {
                finalizer.visit_with(self);
            }
        }
    }

    // === CONDITIONAL-BRANCH OVERRIDES ===

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        stmt.test.visit_with(self);
        self.pass.enter_if();
        if !self.pass.is_budget_exhausted() {
            stmt.cons.visit_with(self);
            if let Some(alt) = &stmt.alt {
                self.pass.enter_else();
                alt.visit_with(self);
            }
        }
        self.pass.exit_if(stmt.span, stmt.alt.is_some());
    }

    fn visit_while_stmt(&mut self, stmt: &WhileStmt) {
        stmt.test.visit_with(self);
        self.visit_loop_body(false, stmt.span, |traversal| {
            stmt.body.visit_with(traversal);
        });
    }

    fn visit_do_while_stmt(&mut self, stmt: &DoWhileStmt) {
        self.visit_loop_body(true, stmt.span, |traversal| stmt.body.visit_with(traversal));
        stmt.test.visit_with(self);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        self.visit_scoped_loop(
            stmt.span,
            |traversal| {
                stmt.init.visit_with(traversal);
                stmt.test.visit_with(traversal);
                stmt.update.visit_with(traversal);
            },
            |traversal| stmt.body.visit_with(traversal),
        );
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        self.visit_scoped_loop(
            stmt.span,
            |traversal| {
                stmt.left.visit_with(traversal);
                stmt.right.visit_with(traversal);
            },
            |traversal| stmt.body.visit_with(traversal),
        );
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        self.visit_scoped_loop(
            stmt.span,
            |traversal| {
                stmt.left.visit_with(traversal);
                stmt.right.visit_with(traversal);
            },
            |traversal| stmt.body.visit_with(traversal),
        );
    }

    fn visit_break_stmt(&mut self, stmt: &BreakStmt) {
        self.pass.break_exit();
        if !self.pass.is_budget_exhausted() {
            stmt.visit_children_with(self);
        }
    }

    fn visit_continue_stmt(&mut self, stmt: &ContinueStmt) {
        self.pass.continue_exit();
        if !self.pass.is_budget_exhausted() {
            stmt.visit_children_with(self);
        }
    }

    fn visit_return_stmt(&mut self, stmt: &ReturnStmt) {
        if !self.pass.is_budget_exhausted() {
            stmt.visit_children_with(self);
        }
        self.pass.mark_unreachable();
    }

    fn visit_throw_stmt(&mut self, stmt: &ThrowStmt) {
        if !self.pass.is_budget_exhausted() {
            stmt.visit_children_with(self);
        }
        self.pass.mark_unreachable();
    }
}
