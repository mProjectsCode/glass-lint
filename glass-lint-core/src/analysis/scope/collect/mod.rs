//! Single-pass collection of conservative lexical and alias facts.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use std::collections::{BTreeMap, BTreeSet};

use history::AssignmentHistory;
use swc_common::{BytePos, Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, BlockStmt, CatchClause, ClassDecl, Expr, FnDecl,
    ForInStmt, ForOfStmt, ForStmt, Function, ImportDecl, ImportSpecifier, ObjectPatProp, Pat,
    SimpleAssignTarget, SwitchStmt, VarDecl, VarDeclKind, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::{
    super::{
        syntax::{
            collect_pat_bindings, function_prototype_builtin, is_function_constructor_member,
            member_chain, member_prop_name, member_root_ident, module_export_name,
        },
        value::BindingVersion,
    },
    AliasAssignment, AliasScope, BindingProvenance, BoundArgument, ScopeId, ScopeKind,
    query::rooted::{RootedExprContext, rooted_expr_chain_with},
};

pub(super) mod aliases;
mod callbacks;
mod constants;
mod history;
mod predeclare;
mod provenance;
mod visitor;

/// Mutable state shared by declaration prepass and source-order collection.
///
/// The prepass establishes lexical binding identity; the normal visitor then
/// reuses that scope tree while recording assignments and supported
/// provenance at each use position.
pub(super) struct AliasCollector {
    /// Lexical scopes in predeclaration/traversal order.
    pub(super) scopes: Vec<AliasScope>,
    /// Current lexical path during AST traversal.
    stack: Vec<usize>,
    /// Assignment events retain source order for use-position provenance.
    pub(super) assignments: Vec<AliasAssignment>,
    /// Latest use-position assignment state per lexical scope.
    latest_assignments: AssignmentHistory,
    /// Property writes retained for flow-aware rooted-member queries.
    pub(super) property_assignments: Vec<PropertyAliasAssignment>,
    /// Writes that invalidate a rooted receiver/property identity.
    pub(super) rooted_property_mutations: Vec<RootedPropertyMutation>,
    /// Dynamic `eval` sites that make local provenance conservative.
    pub(super) dynamic_evals: Vec<(ScopeId, Span)>,
    /// Function scopes and their parameter patterns by visible name.
    pub(super) function_scopes: BTreeMap<(ScopeId, String), (ScopeId, Vec<Pat>)>,
    /// Aliases that point to a locally declared helper function.
    pub(super) function_aliases: BTreeMap<(ScopeId, String), ScopeId>,
    /// Calls retained for the later, scope-aware helper parameter pass.
    calls: Vec<(ScopeId, String, Vec<Option<BindingProvenance>>)>,
    /// Proven callback arguments installed when an inline function is entered.
    inline_parameters: BTreeMap<BytePos, BTreeMap<String, BindingProvenance>>,
    /// `var`-bound objects whose mutation prevents constant projection.
    pub(super) mutable_static_objects: BTreeSet<(ScopeId, String)>,
    reuse_scopes: bool,
    predeclared_scope_order: Vec<usize>,
    next_predeclared_scope: usize,
    #[cfg(test)]
    scope_reuse_steps: usize,
}

#[derive(Debug, Clone)]
/// Assignment of a rooted member/property alias at a source position.
pub(super) struct PropertyAliasAssignment {
    pub(super) span: Span,
    pub(super) scope: ScopeId,
    pub(super) property: String,
    pub(super) receiver: swc_ecma_ast::Ident,
    pub(super) target: Option<String>,
}

#[derive(Debug, Clone)]
/// Mutation that can invalidate a rooted property provenance.
pub(super) struct RootedPropertyMutation {
    pub(super) span: Span,
    pub(super) scope: ScopeId,
    pub(super) receiver: String,
    pub(super) property: Option<String>,
}

impl AliasCollector {
    /// Initialize an empty collector with a program-level lexical scope.
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
            latest_assignments: AssignmentHistory::default(),
            property_assignments: Vec::new(),
            rooted_property_mutations: Vec::new(),
            dynamic_evals: Vec::new(),
            function_scopes: BTreeMap::new(),
            function_aliases: BTreeMap::new(),
            calls: Vec::new(),
            inline_parameters: BTreeMap::new(),
            mutable_static_objects: BTreeSet::new(),
            reuse_scopes: false,
            predeclared_scope_order: Vec::new(),
            next_predeclared_scope: 0,
            #[cfg(test)]
            scope_reuse_steps: 0,
        }
    }

    /// Populate the same scope tree that the fact collector will use, but do
    /// only declaration work.  JavaScript lexical bindings are visible for
    /// the whole lexical scope (with TDZ handled as an unresolved/local fact),
    /// and `var`/function declarations are hoisted.  The old collector made
    /// visibility depend on whether traversal had reached the declaration,
    /// which incorrectly treated an earlier use as a global.
    pub fn predeclare(&mut self, program: &swc_ecma_ast::Program) {
        let mut visitor = predeclare::PredeclareVisitor { collector: self };
        program.visit_children_with(&mut visitor);
        self.reuse_scopes = true;
        self.next_predeclared_scope = 0;
        #[cfg(test)]
        {
            self.scope_reuse_steps = 0;
        }
    }

    /// Return the innermost scope currently being traversed.
    fn current_scope(&self) -> ScopeId {
        ScopeId::from(self.stack.last().copied().unwrap_or(0))
    }

    /// Bundlers emit these wrappers around CommonJS imports. They are
    /// recognized only while the wrapper name is itself unbound; a local
    /// function with the same spelling must remain local.
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

    /// Map a declaration kind to its lexical or function binding scope.
    fn binding_scope(&self, kind: VarDeclKind) -> ScopeId {
        if kind != VarDeclKind::Var {
            return self.current_scope();
        }
        // `var` is function-scoped, unlike `let` and `const`, so skip nested
        // blocks until the enclosing function or program scope is reached.
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
            .map_or_else(|| ScopeId::from(0), ScopeId::from)
    }

    /// Insert a declaration's initial provenance into a lexical scope.
    pub fn insert(
        &mut self,
        scope: ScopeId,
        name: impl Into<String>,
        provenance: BindingProvenance,
    ) {
        self.scopes[scope.index()]
            .bindings
            .insert(name.into(), provenance);
    }

    fn insert_local(&mut self, scope: ScopeId, name: impl Into<String>) {
        self.insert(scope, name, BindingProvenance::Local);
    }

    /// Append a source-ordered assignment version and update latest state.
    pub fn record_assignment(
        &mut self,
        span: Span,
        scope: ScopeId,
        name: String,
        provenance: BindingProvenance,
    ) {
        let version = BindingVersion(
            u32::try_from(
                self.assignments
                    .iter()
                    .filter(|assignment| assignment.scope == scope && assignment.name == name)
                    .count()
                    .saturating_add(1),
            )
            .unwrap_or(u32::MAX),
        );
        self.latest_assignments
            .record(scope, name.clone(), provenance.clone());
        self.assignments.push(AliasAssignment {
            span,
            scope,
            name,
            version,
            provenance,
        });
    }

    /// Enter a predeclared scope, asserting traversal order matches the pass.
    fn push_scope(&mut self, span: Span, kind: ScopeKind) {
        if self.reuse_scopes {
            let parent = self.current_scope();
            let Some(&index) = self
                .predeclared_scope_order
                .get(self.next_predeclared_scope)
            else {
                panic!("normal traversal entered more scopes than predeclaration");
            };
            self.next_predeclared_scope += 1;
            let matches_predeclared = self.scopes[index].parent == Some(parent)
                && self.scopes[index].span == span
                && self.scopes[index].kind == kind;
            debug_assert!(
                matches_predeclared,
                "normal traversal must consume its matching predeclared scope"
            );
            assert!(
                matches_predeclared,
                "normal traversal scope order diverged from predeclaration"
            );
            self.stack.push(index);
            #[cfg(test)]
            {
                self.scope_reuse_steps += 1;
            }
            return;
        }
        let index = self.scopes.len();
        let parent = self.current_scope();
        self.scopes.push(AliasScope {
            span,
            depth: self.stack.len(),
            kind,
            parent: Some(parent),
            bindings: BTreeMap::new(),
        });
        self.predeclared_scope_order.push(index);
        self.stack.push(index);
    }

    /// Leave the current nested scope without popping the program scope.
    fn pop_scope(&mut self) {
        if self.stack.len() <= 1 {
            debug_assert!(false, "attempted to pop the program scope");
            return;
        }
        let _ = self.stack.pop();
    }

    /// Register every binding introduced by a declaration pattern as local.
    fn insert_pat_locals(&mut self, scope: ScopeId, pat: &Pat) {
        let mut bindings = BTreeSet::new();
        collect_pat_bindings(pat, &mut bindings);
        for binding in bindings {
            self.insert_local(scope, binding);
        }
    }

    fn visible_binding(&self, name: &str) -> Option<&BindingProvenance> {
        // Prefer assignments over declarations inside each scope: while
        // collecting source order, `latest_assignments` is exactly the state
        // visible at the current AST position.
        for scope in self.stack.iter().rev().copied().map(ScopeId::from) {
            if let Some(assignment) = self.latest_assignments.get(scope, name) {
                return Some(assignment);
            }
            if let Some(binding) = self.scopes[scope.index()].bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn visible_binding_scope(&self, name: &str) -> Option<ScopeId> {
        self.stack
            .iter()
            .rev()
            .copied()
            .map(ScopeId::from)
            .find(|scope| {
                self.latest_assignments.contains(*scope, name)
                    || self.scopes[scope.index()].bindings.contains_key(name)
            })
    }

    fn is_unbound(&self, name: &str) -> bool {
        self.visible_binding(name).is_none()
    }

    fn rooted_expr_name(&self, expr: &Expr) -> Option<String> {
        rooted_expr_chain_with(self, expr)
    }

    fn invalidate_member_root(&mut self, member: &swc_ecma_ast::MemberExpr, span: Span) {
        let Some(root) = member_root_ident(member) else {
            return;
        };
        if !matches!(
            self.visible_binding(root.sym.as_ref()),
            Some(
                BindingProvenance::StaticStringArray(_)
                    | BindingProvenance::StaticObjectKeys(_)
                    | BindingProvenance::StaticObjectValues(_)
            )
        ) {
            return;
        }
        let Some(scope) = self.stack.iter().rev().find(|scope| {
            self.scopes[**scope]
                .bindings
                .contains_key(root.sym.as_ref())
        }) else {
            return;
        };
        self.record_assignment(
            span,
            ScopeId::from(*scope),
            root.sym.to_string(),
            BindingProvenance::Local,
        );
    }

    /// Copy parameter patterns into the function metadata used by the later
    /// call-site projection pass. Keeping this conversion here makes the
    /// collector's function metadata independent of SWC's parameter wrapper.
    fn function_parameters(function: &Function) -> Vec<Pat> {
        function
            .params
            .iter()
            .map(|parameter| parameter.pat.clone())
            .collect()
    }

    fn arrow_parameters(arrow: &ArrowExpr) -> Vec<Pat> {
        arrow.params.clone()
    }

    fn register_function_expression(&mut self, name: String, expr: &Expr) -> bool {
        let declaration_scope = self.current_scope();
        match expr {
            Expr::Arrow(arrow) => {
                let parameters = Self::arrow_parameters(arrow);
                self.push_scope(arrow.span, ScopeKind::Function);
                let scope = self.current_scope();
                for param in &arrow.params {
                    self.insert_pat_locals(scope, param);
                }
                self.function_scopes
                    .insert((declaration_scope, name), (scope, parameters));
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
                self.function_scopes
                    .insert((declaration_scope, name), (scope, parameters));
                function_expr.function.decorators.visit_with(self);
                function_expr.function.body.visit_with(self);
                self.pop_scope();
                true
            }
            Expr::Paren(paren) => self.register_function_expression(name, &paren.expr),
            _ => false,
        }
    }
}

impl RootedExprContext for AliasCollector {
    fn rooted_ident_chain(&self, ident: &swc_ecma_ast::Ident) -> Option<String> {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(
                BindingProvenance::ValueAlias { target }
                | BindingProvenance::BoundCallable { target, .. },
            ) => Some(target.to_string()),
            Some(_) => None,
            None => Some(ident.sym.to_string()),
        }
    }

    fn rooted_member_chain(&self, member: &swc_ecma_ast::MemberExpr) -> Option<String> {
        if is_function_constructor_member(member)
            && function_prototype_builtin(&member.obj).is_none_or(|name| self.is_unbound(name))
        {
            return Some("Function".to_string());
        }
        let object = self.rooted_expr_name(&member.obj)?;
        let property = member_prop_name(&member.prop)?;
        Some(format!("{object}.{property}"))
    }
}

#[cfg(test)]
mod tests {
    use swc_ecma_visit::VisitWith;

    use super::*;

    fn collect(source: &str) -> AliasCollector {
        let parsed = crate::parse(source, "scope-collector.js").expect("source should parse");
        let mut collector = AliasCollector::new(parsed.program.span());
        collector.predeclare(&parsed.program);
        parsed.program.visit_children_with(&mut collector);
        assert_eq!(
            collector.next_predeclared_scope,
            collector.predeclared_scope_order.len()
        );
        assert_eq!(
            collector.scope_reuse_steps,
            collector.predeclared_scope_order.len()
        );
        collector
    }

    fn scope_fingerprint(collector: &AliasCollector) -> Vec<String> {
        collector
            .scopes
            .iter()
            .map(|scope| {
                format!(
                    "parent={:?} depth={} kind={:?} span=({}, {}) bindings={:?}",
                    scope.parent,
                    scope.depth,
                    scope.kind,
                    scope.span.lo.0,
                    scope.span.hi.0,
                    scope.bindings
                )
            })
            .collect()
    }

    #[test]
    fn preserves_scope_order_for_all_scope_constructs() {
        let source = r"
            function outer(parameter) {
                { let block = parameter; }
                for (let index = 0; index < 1; index++) {
                    (() => { let nested = index; })();
                }
                for (const item of items) { function loopFunction() {} }
                for (const key in object) { key; }
                switch (parameter) {
                    case 0: { let caseValue = parameter; break; }
                    default: break;
                }
                try { throw parameter; }
                catch (error) { const caught = error; }
                with (context) { value; }
                const functionValue = function named(value) { return value; };
                const arrow = value => { return value; };
            }
        ";
        let first = collect(source);
        let second = collect(source);

        assert_eq!(scope_fingerprint(&first), scope_fingerprint(&second));
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Function)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Block)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Dynamic)
        );
        assert!(
            first
                .scopes
                .iter()
                .any(|scope| scope.kind == ScopeKind::Function && scope.depth > 2)
        );
    }

    #[test]
    fn reuses_same_span_same_kind_siblings_by_order() {
        let parsed = crate::parse("value;", "same-span.js").expect("source should parse");
        let span = parsed.program.span();
        let mut collector = AliasCollector::new(span);

        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        collector.pop_scope();
        collector.reuse_scopes = true;
        collector.next_predeclared_scope = 0;

        collector.push_scope(span, ScopeKind::Block);
        let first = collector.current_scope();
        collector.pop_scope();
        collector.push_scope(span, ScopeKind::Block);
        let second = collector.current_scope();

        assert_eq!((first, second), (ScopeId::from(1), ScopeId::from(2)));
        assert_eq!(collector.scope_reuse_steps, 2);
    }

    fn sibling_scope_steps(count: usize) -> usize {
        let source = (0..count)
            .map(|index| format!("{{ let value{index} = {index}; }}"))
            .collect::<Vec<_>>()
            .join("\n");
        let collector = collect(&source);
        collector.scope_reuse_steps
    }

    #[test]
    fn many_sibling_scopes_use_one_cursor_step_each() {
        let one = sibling_scope_steps(128);
        let two = sibling_scope_steps(256);

        assert_eq!(one, 128);
        assert_eq!(two, one * 2);
    }
}
