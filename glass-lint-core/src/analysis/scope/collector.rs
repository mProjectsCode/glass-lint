//! Single-pass collection of conservative lexical and alias facts.
//!
//! The visitor records declarations as it enters scopes and assignments in
//! source order. It deliberately models only callback forms whose argument-to-
//! parameter mapping is unambiguous; uncertain calls leave parameters local.

use std::collections::{BTreeMap, BTreeSet};

use swc_common::{BytePos, Span, Spanned};
use swc_ecma_ast::{
    ArrowExpr, AssignExpr, AssignTarget, BlockStmt, CallExpr, Callee, CatchClause, ClassDecl, Expr,
    FnDecl, ForInStmt, ForOfStmt, ForStmt, Function, ImportDecl, ImportSpecifier, Lit,
    ObjectPatProp, Pat, SimpleAssignTarget, SwitchStmt, VarDecl, VarDeclKind, WithStmt,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::super::ast::{
    collect_pat_bindings, function_prototype_builtin, is_function_constructor_member, member_chain,
    member_prop_name, member_root_ident, module_export_name, prop_name,
};
use super::super::constant::{self, ConstValue};
use super::super::value::BindingVersion;
use super::aliases::{collect_assignment_aliases, collect_require_aliases, collect_value_aliases};
use super::rooted::{RootedExprContext, rooted_expr_chain_with};
use super::{AliasAssignment, AliasScope, BindingProvenance, BoundArgument, ScopeKind};

mod callbacks;
mod constants;
mod predeclare;

pub struct AliasCollector {
    pub scopes: Vec<AliasScope>,
    stack: Vec<usize>,
    pub assignments: Vec<AliasAssignment>,
    latest_assignments: BTreeMap<usize, BTreeMap<String, BindingProvenance>>,
    pub property_assignments: Vec<PropertyAliasAssignment>,
    pub dynamic_evals: Vec<(usize, Span)>,
    pub(super) function_scopes: BTreeMap<(usize, String), (usize, Vec<Pat>)>,
    pub(super) function_aliases: BTreeMap<(usize, String), usize>,
    calls: Vec<(usize, String, Vec<Option<BindingProvenance>>)>,
    inline_parameters: BTreeMap<BytePos, BTreeMap<String, BindingProvenance>>,
    pub mutable_static_objects: BTreeSet<(usize, String)>,
    reuse_scopes: bool,
    predeclared_scope_order: Vec<usize>,
    next_predeclared_scope: usize,
    #[cfg(test)]
    scope_reuse_steps: usize,
}

#[derive(Debug, Clone)]
pub(super) struct PropertyAliasAssignment {
    pub(super) span: Span,
    pub(super) scope: usize,
    pub(super) property: String,
    pub(super) receiver: swc_ecma_ast::Ident,
    pub(super) target: Option<String>,
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

    fn current_scope(&self) -> usize {
        self.stack.last().copied().unwrap_or(0)
    }

    fn binding_scope(&self, kind: VarDeclKind) -> usize {
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
            .unwrap_or(0)
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
            .entry(scope)
            .or_default()
            .insert(name.clone(), provenance.clone());
        self.assignments.push(AliasAssignment {
            span,
            scope,
            name,
            version,
            provenance,
        });
    }

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

    fn pop_scope(&mut self) {
        if self.stack.len() <= 1 {
            debug_assert!(false, "attempted to pop the program scope");
            return;
        }
        let _ = self.stack.pop();
    }

    fn insert_pat_locals(&mut self, scope: usize, pat: &Pat) {
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

    fn visible_binding_scope(&self, name: &str) -> Option<usize> {
        self.stack.iter().rev().copied().find(|scope| {
            self.latest_assignments
                .get(scope)
                .is_some_and(|assignments| assignments.contains_key(name))
                || self.scopes[*scope].bindings.contains_key(name)
        })
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
                | BindingProvenance::BoundCallable { .. }
                | BindingProvenance::BoundModuleCallable { .. }
                | BindingProvenance::ReturnedObject { .. }
                | BindingProvenance::StaticString(_)
                | BindingProvenance::StaticNumber(_)
                | BindingProvenance::StaticStringArray(_)
                | BindingProvenance::StaticObjectKeys(_)
                | BindingProvenance::StaticObjectValues(_) => None,
            },
            Expr::Member(member) => {
                match self.module_alias_provenance(&member.obj)? {
                    BindingProvenance::ModuleNamespace { module } => {
                        Some(BindingProvenance::ModuleExport {
                            module: module.clone(),
                            export: member_prop_name(&member.prop)?,
                        })
                    }
                    // Binding an export retains the export's callable provenance.
                    provenance @ BindingProvenance::ModuleExport { .. }
                        if member_prop_name(&member.prop).as_deref() == Some("bind") =>
                    {
                        Some(provenance)
                    }
                    _ => None,
                }
            }
            Expr::Call(call) => self
                .require_module_name(call)
                .map(|module| BindingProvenance::ModuleNamespace { module })
                .or_else(|| {
                    let Callee::Expr(callee) = &call.callee else {
                        return None;
                    };
                    let Expr::Member(member) = &**callee else {
                        return None;
                    };
                    (member_prop_name(&member.prop).as_deref() == Some("bind"))
                        .then(|| self.module_alias_provenance(&member.obj))
                        .flatten()
                }),
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
        match constant::evaluate(init, self) {
            ConstValue::String(value) => Some(BindingProvenance::StaticString(value)),
            ConstValue::NonNegativeInteger(value) => Some(BindingProvenance::StaticNumber(value)),
            ConstValue::Array(values) => Some(BindingProvenance::StaticStringArray(
                values
                    .into_iter()
                    .map(|value| value.string().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
            )),
            ConstValue::Object(values) => Some(BindingProvenance::StaticObjectKeys(
                values.keys().cloned().collect(),
            )),
            ConstValue::Unknown => None,
        }
    }

    fn argument_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        self.module_alias_provenance(expr)
            .or_else(|| self.returned_object_provenance(expr))
            .or_else(|| match expr {
                Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                    provenance @ BindingProvenance::StaticObjectValues(_) => {
                        Some(provenance.clone())
                    }
                    _ => None,
                },
                _ => None,
            })
            .or_else(|| self.static_object_values(expr))
            .or_else(|| self.const_provenance(expr))
            .or_else(|| {
                self.rooted_expr_name(expr)
                    .map(|target| BindingProvenance::ValueAlias {
                        target: target.into(),
                    })
            })
    }

    fn bound_callable_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        let Expr::Call(call) = expr else {
            return None;
        };
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(member) = &**callee else {
            return None;
        };
        if member_prop_name(&member.prop).as_deref() != Some("bind") {
            return None;
        }
        let target = self.rooted_expr_name(&member.obj)?;
        let bound_arguments = call
            .args
            .iter()
            .skip(1)
            .map(|argument| {
                self.const_provenance(&argument.expr)
                    .and_then(|provenance| match provenance {
                        BindingProvenance::StaticString(value) => {
                            Some(BoundArgument::StaticString(value))
                        }
                        _ => None,
                    })
                    .or_else(|| {
                        self.rooted_expr_name(&argument.expr)
                            .map(|value| BoundArgument::RootedExpression(value.into()))
                    })
            })
            .collect();
        match self.module_alias_provenance(&member.obj) {
            Some(BindingProvenance::ModuleExport { module, export }) => {
                Some(BindingProvenance::BoundModuleCallable {
                    module,
                    export,
                    bound_arguments,
                })
            }
            _ => Some(BindingProvenance::BoundCallable {
                target: target.into(),
                bound_arguments,
            }),
        }
    }

    fn returned_object_provenance(&self, expr: &Expr) -> Option<BindingProvenance> {
        match expr {
            Expr::Call(call) => {
                let Callee::Expr(callee) = &call.callee else {
                    return None;
                };
                if let Expr::Member(member) = &**callee
                    && member_prop_name(&member.prop).as_deref() == Some("bind")
                {
                    return None;
                }
                let source = self.rooted_expr_name(callee)?;
                source
                    .contains('.')
                    .then_some(BindingProvenance::ReturnedObject {
                        source: source.into(),
                    })
            }
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                BindingProvenance::ReturnedObject { source } => {
                    Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    })
                }
                _ => None,
            },
            Expr::Member(member) => {
                if let Expr::Ident(ident) = &*member.obj
                    && let Some(BindingProvenance::ReturnedObject { source }) =
                        self.visible_binding(ident.sym.as_ref())
                {
                    return Some(BindingProvenance::ReturnedObject {
                        source: source.clone(),
                    });
                }
                self.rooted_expr_name(expr)
                    .map(|source| BindingProvenance::ReturnedObject {
                        source: source.into(),
                    })
            }
            Expr::Paren(paren) => self.returned_object_provenance(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.returned_object_provenance(expr)),
            _ => None,
        }
    }

    fn static_object_values(&self, expr: &Expr) -> Option<BindingProvenance> {
        let Expr::Object(object) = expr else {
            return None;
        };
        let mut values = BTreeMap::new();
        for property in &object.props {
            let swc_ecma_ast::PropOrSpread::Prop(property) = property else {
                return None;
            };
            let swc_ecma_ast::Prop::KeyValue(property) = &**property else {
                return None;
            };
            let target = self.rooted_expr_name(&property.value)?;
            values.insert(prop_name(&property.key)?, target.into());
        }
        Some(BindingProvenance::StaticObjectValues(values))
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
        self.record_assignment(span, *scope, root.sym.to_string(), BindingProvenance::Local);
    }

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

    pub fn parameter_aliases(&self) -> BTreeMap<(usize, String), BindingProvenance> {
        let mut aliases = BTreeMap::<(usize, String), Option<BindingProvenance>>::new();
        for (caller_scope, callee, arguments) in &self.calls {
            let Some((scope, parameters)) = self.function_for_call(*caller_scope, callee) else {
                continue;
            };
            for (index, parameter) in parameters.iter().enumerate() {
                let mut projected = BTreeMap::new();
                if *caller_scope != *scope
                    && let Some(Some(target)) = arguments.get(index)
                {
                    project_parameter_pattern(parameter, target, &mut projected);
                }
                for name in parameter_binding_names(parameter) {
                    let target = projected.get(&name).cloned();
                    let entry = aliases.entry((*scope, name)).or_insert(target.clone());
                    if *entry != target {
                        *entry = None;
                    }
                }
            }
            if arguments.len() != parameters.len() {
                for parameter in parameters {
                    for name in parameter_binding_names(parameter) {
                        aliases.insert((*scope, name), None);
                    }
                }
            }
        }
        aliases
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect()
    }

    fn function_for_call(&self, mut scope: usize, name: &str) -> Option<&(usize, Vec<Pat>)> {
        loop {
            if let Some(function) = self.function_scopes.get(&(scope, name.to_string())) {
                return Some(function);
            }
            scope = self.scopes.get(scope)?.parent?;
        }
    }

    fn function_scope_for_name(&self, name: &str) -> Option<usize> {
        let mut scope = self.current_scope();
        loop {
            if let Some((function, _)) = self.function_scopes.get(&(scope, name.to_string())) {
                return Some(*function);
            }
            scope = self.scopes.get(scope)?.parent?;
        }
    }
}

fn parameter_binding_names(pattern: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_parameter_binding_names(pattern, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_parameter_binding_names(pattern: &Pat, names: &mut Vec<String>) {
    match pattern {
        Pat::Ident(ident) => names.push(ident.id.sym.to_string()),
        Pat::Assign(assign) => collect_parameter_binding_names(&assign.left, names),
        Pat::Object(object) => {
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        collect_parameter_binding_names(&property.value, names)
                    }
                    ObjectPatProp::Assign(property) => names.push(property.key.sym.to_string()),
                    ObjectPatProp::Rest(property) => {
                        collect_parameter_binding_names(&property.arg, names)
                    }
                }
            }
        }
        Pat::Array(array) => {
            for element in array.elems.iter().flatten() {
                collect_parameter_binding_names(element, names);
            }
        }
        Pat::Rest(rest) => collect_parameter_binding_names(&rest.arg, names),
        Pat::Expr(_) | Pat::Invalid(_) => {}
    }
}

pub(super) fn project_parameter_pattern(
    pattern: &Pat,
    value: &BindingProvenance,
    output: &mut BTreeMap<String, BindingProvenance>,
) {
    match pattern {
        Pat::Ident(ident) => {
            output.insert(ident.id.sym.to_string(), value.clone());
        }
        Pat::Assign(assign) => project_parameter_pattern(&assign.left, value, output),
        Pat::Object(object) => {
            let BindingProvenance::StaticObjectValues(values) = value else {
                return;
            };
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        let Some(key) = prop_name(&property.key) else {
                            continue;
                        };
                        let Some(target) = values.get(&key) else {
                            continue;
                        };
                        project_parameter_pattern(
                            &property.value,
                            &BindingProvenance::ValueAlias {
                                target: target.clone(),
                            },
                            output,
                        );
                    }
                    ObjectPatProp::Assign(property) => {
                        if let Some(target) = values.get(property.key.sym.as_ref()) {
                            output.insert(
                                property.key.sym.to_string(),
                                BindingProvenance::ValueAlias {
                                    target: target.clone(),
                                },
                            );
                        }
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
        }
        Pat::Array(_) | Pat::Rest(_) | Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

impl RootedExprContext for AliasCollector {
    fn rooted_ident_chain(&self, ident: &swc_ecma_ast::Ident) -> Option<String> {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(BindingProvenance::ValueAlias { target }) => Some(target.to_string()),
            Some(BindingProvenance::BoundCallable { target, .. }) => Some(target.to_string()),
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
        if let Expr::Ident(root) = &*member.obj
            && root.sym == *"globalThis"
            && self.is_unbound("globalThis")
        {
            return member_prop_name(&member.prop);
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
                        collect_value_aliases(&property.value, "Function", scope, self);
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
                collect_require_aliases(&declarator.name, module.clone(), scope, self);
            } else if let Some(init) = declarator.init.as_deref()
                && let Some(module) = self.require_module_expr_name(init)
            {
                collect_require_aliases(&declarator.name, module, scope, self);
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
                collect_value_aliases(&declarator.name, &target, scope, self);
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

#[cfg(test)]
mod tests {
    use super::*;
    use swc_ecma_visit::VisitWith;

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
        let source = r#"
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
        "#;
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

        assert_eq!((first, second), (1, 2));
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
