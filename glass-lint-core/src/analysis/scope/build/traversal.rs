use swc_common::{Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, BlockStmt, CallExpr, CatchClause, ClassDecl, FnDecl, ForInStmt,
    ForOfStmt, ForStmt, Function, Ident, ImportDecl, Lit, MemberExpr, Pat, PropName, SwitchStmt,
    VarDecl, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::{ScopeId, ScopeKind};

/// Phase-specific policy for scope traversal.
///
/// Each phase owns its own scope stack, `push_scope`/`pop_scope` logic, and
/// non-scope-forming visit overrides. Default trait methods are no-ops so a
/// pass overrides only the methods it needs.
pub(in crate::analysis::scope) trait ScopePass {
    fn push_scope(&mut self, span: Span, kind: ScopeKind);
    fn pop_scope(&mut self);
    fn current_scope(&self) -> ScopeId;

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
}

impl<P: ScopePass> Visit for ScopeTraversal<P> {
    fn visit_ident(&mut self, ident: &Ident) {
        self.pass.visit_ident(ident);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        self.pass.visit_member_expr(member);
        member.visit_children_with(self);
    }

    fn visit_prop_name(&mut self, prop: &PropName) {
        self.pass.visit_prop_name(prop);
        prop.visit_children_with(self);
    }

    fn visit_lit(&mut self, lit: &Lit) {
        self.pass.visit_lit(lit);
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        self.pass.visit_import_decl(import);
        import.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, decl: &VarDecl) {
        self.pass.visit_var_decl(decl);
        decl.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr) {
        self.pass.visit_assign_expr(expr);
        expr.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        self.pass.visit_call_expr(call);
        call.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, decl: &ClassDecl) {
        self.pass.visit_class_decl(decl);
        decl.visit_children_with(self);
    }

    // === SCOPE-FORMING METHODS ===

    fn visit_fn_decl(&mut self, decl: &FnDecl) {
        let parent = self.pass.current_scope();
        self.pass.before_fn_decl(decl, parent);
        self.pass
            .push_scope(decl.function.span, ScopeKind::Function);
        let scope = self.pass.current_scope();
        self.pass.after_fn_decl(decl, scope);
        for param in &decl.function.params {
            param.pat.visit_with(self);
        }
        decl.function.decorators.visit_with(self);
        decl.function.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_function(&mut self, func: &Function) {
        self.pass.push_scope(func.span, ScopeKind::Function);
        let scope = self.pass.current_scope();
        self.pass.after_function(func, scope);
        for param in &func.params {
            param.pat.visit_with(self);
        }
        func.decorators.visit_with(self);
        func.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        self.pass.push_scope(arrow.span, ScopeKind::Function);
        let scope = self.pass.current_scope();
        self.pass.after_arrow(arrow, scope);
        for param in &arrow.params {
            param.visit_with(self);
        }
        arrow.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        self.pass.push_scope(block.span, ScopeKind::Block);
        block.stmts.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        self.pass.push_scope(stmt.span, ScopeKind::Block);
        stmt.init.visit_with(self);
        stmt.test.visit_with(self);
        stmt.update.visit_with(self);
        stmt.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, stmt: &ForInStmt) {
        self.pass.push_scope(stmt.span, ScopeKind::Block);
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_for_of_stmt(&mut self, stmt: &ForOfStmt) {
        self.pass.push_scope(stmt.span, ScopeKind::Block);
        stmt.left.visit_with(self);
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        stmt.discriminant.visit_with(self);
        self.pass.push_scope(stmt.span, ScopeKind::Block);
        stmt.cases.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_with_stmt(&mut self, stmt: &WithStmt) {
        stmt.obj.visit_with(self);
        self.pass.push_scope(stmt.body.span(), ScopeKind::Dynamic);
        stmt.body.visit_with(self);
        self.pass.pop_scope();
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause) {
        self.pass.push_scope(clause.span, ScopeKind::Block);
        if let Some(param) = &clause.param {
            self.pass.visit_catch_param(param);
        }
        clause.body.stmts.visit_with(self);
        self.pass.pop_scope();
    }
}
