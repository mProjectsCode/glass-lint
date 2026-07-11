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
    member_prop_name, member_root_ident, module_export_name, prop_name, static_string,
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
    pub dynamic_evals: Vec<(usize, Span)>,
    functions: BTreeMap<String, (usize, Vec<Pat>)>,
    calls: Vec<(String, Vec<Option<BindingProvenance>>)>,
    inline_parameters: BTreeMap<BytePos, BTreeMap<String, BindingProvenance>>,
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
            functions: BTreeMap::new(),
            calls: Vec::new(),
            inline_parameters: BTreeMap::new(),
        }
    }

    fn current_scope(&self) -> usize {
        *self.stack.last().expect("program scope is always present")
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
        if let Expr::Lit(Lit::Num(number)) = init
            && number.value.is_finite()
            && number.value >= 0.0
            && number.value.fract() == 0.0
        {
            return Some(BindingProvenance::StaticNumber(number.value as usize));
        }
        if let Some(value) = self.static_string_value(init) {
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
        if let Some(keys) = self.static_object_keys(init) {
            return Some(BindingProvenance::StaticObjectKeys(keys));
        }
        None
    }

    fn static_string_value(&self, expr: &Expr) -> Option<String> {
        static_string(expr).or_else(|| match expr {
            Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                BindingProvenance::StaticString(value) => Some(value.clone()),
                _ => None,
            },
            Expr::Tpl(template) => {
                let mut value = String::new();
                for (index, quasi) in template.quasis.iter().enumerate() {
                    value.push_str(&quasi.raw);
                    if let Some(expr) = template.exprs.get(index) {
                        value.push_str(&self.static_string_value(expr)?);
                    }
                }
                Some(value)
            }
            Expr::Bin(binary) if binary.op == swc_ecma_ast::BinaryOp::Add => Some(format!(
                "{}{}",
                self.static_string_value(&binary.left)?,
                self.static_string_value(&binary.right)?
            )),
            Expr::Paren(paren) => self.static_string_value(&paren.expr),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expr| self.static_string_value(expr)),
            _ => None,
        })
    }

    fn static_object_keys(&self, expr: &Expr) -> Option<Vec<String>> {
        if let Expr::Object(object) = expr {
            let mut keys = Vec::new();
            for property in &object.props {
                match property {
                    swc_ecma_ast::PropOrSpread::Prop(property) => match &**property {
                        swc_ecma_ast::Prop::Shorthand(ident) => keys.push(ident.sym.to_string()),
                        swc_ecma_ast::Prop::KeyValue(property) => {
                            keys.push(prop_name(&property.key)?)
                        }
                        swc_ecma_ast::Prop::Assign(property) => {
                            keys.push(property.key.sym.to_string())
                        }
                        // Accessors and methods can execute code, so are deliberately unknown.
                        _ => return None,
                    },
                    swc_ecma_ast::PropOrSpread::Spread(spread) => match &*spread.expr {
                        Expr::Ident(ident) => match self.visible_binding(ident.sym.as_ref())? {
                            BindingProvenance::StaticObjectKeys(existing) => {
                                keys.extend(existing.clone())
                            }
                            _ => return None,
                        },
                        _ => return None,
                    },
                }
            }
            keys.sort();
            keys.dedup();
            return Some(keys);
        }
        let Expr::Call(call) = expr else { return None };
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(member) = &**callee else {
            return None;
        };
        let Expr::Ident(object) = &*member.obj else {
            return None;
        };
        if object.sym != *"Object"
            || !self.is_unbound("Object")
            || member_prop_name(&member.prop).as_deref() != Some("assign")
            || call.args.is_empty()
            || !matches!(&*call.args[0].expr, Expr::Object(object) if object.props.is_empty())
        {
            return None;
        }
        let mut keys = Vec::new();
        for argument in call.args.iter().skip(1) {
            keys.extend(self.static_object_keys(&argument.expr)?);
        }
        keys.sort();
        keys.dedup();
        Some(keys)
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
                    .map(|target| BindingProvenance::ValueAlias { target })
            })
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
                    .then_some(BindingProvenance::ReturnedObject { source })
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
                    .map(|source| BindingProvenance::ReturnedObject { source })
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
            values.insert(prop_name(&property.key)?, target);
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

    fn bind_inline_parameters<'a>(
        &mut self,
        span: Span,
        parameters: impl IntoIterator<Item = &'a Pat>,
        arguments: impl IntoIterator<Item = Option<BindingProvenance>>,
    ) {
        // Inline callbacks are visited after their call expression is seen.
        // Stash the proven argument facts by span so they can be installed when
        // the callback's lexical scope is entered.
        let mut bindings = BTreeMap::new();
        for (parameter, argument) in parameters.into_iter().zip(arguments) {
            if let Some(argument) = argument {
                project_parameter_pattern(parameter, &argument, &mut bindings);
            }
        }
        if !bindings.is_empty() {
            self.inline_parameters.insert(span.lo, bindings);
        }
    }

    fn record_modeled_callbacks(&mut self, call: &CallExpr) {
        let Callee::Expr(callee) = &call.callee else {
            return;
        };
        let callee = match &**callee {
            Expr::Paren(paren) => &*paren.expr,
            callee => callee,
        };
        let arguments = || {
            call.args
                .iter()
                .map(|arg| self.argument_provenance(&arg.expr))
                .collect::<Vec<_>>()
        };
        match callee {
            Expr::Arrow(arrow) => {
                self.bind_inline_parameters(arrow.span, arrow.params.iter(), arguments());
                return;
            }
            Expr::Fn(function) => {
                self.bind_inline_parameters(
                    function.function.span,
                    function.function.params.iter().map(|param| &param.pat),
                    arguments(),
                );
                return;
            }
            _ => {}
        }
        let Expr::Member(member) = callee else { return };
        let Some(method) = member_prop_name(&member.prop) else {
            return;
        };
        if method == "forEach" {
            let Expr::Array(array) = &*member.obj else {
                return;
            };
            let elements = array
                .elems
                .iter()
                .map(Option::as_ref)
                .collect::<Option<Vec<_>>>();
            let Some(elements) = elements else { return };
            let Some(first) = elements.first() else {
                return;
            };
            let value = self.argument_provenance(&first.expr);
            if elements
                .iter()
                .skip(1)
                .any(|element| self.argument_provenance(&element.expr) != value)
            {
                return;
            }
            let Some(Expr::Arrow(callback)) = call.args.first().map(|arg| &*arg.expr) else {
                return;
            };
            self.bind_inline_parameters(callback.span, callback.params.iter(), [value]);
            return;
        }
        if method != "then" || !self.is_unbound("Promise") {
            return;
        }
        let Expr::Call(resolve) = &*member.obj else {
            return;
        };
        let Callee::Expr(resolve_callee) = &resolve.callee else {
            return;
        };
        let Expr::Member(resolve_member) = &**resolve_callee else {
            return;
        };
        let Expr::Ident(promise) = &*resolve_member.obj else {
            return;
        };
        if promise.sym != *"Promise"
            || member_prop_name(&resolve_member.prop).as_deref() != Some("resolve")
        {
            return;
        }
        let Some(Expr::Arrow(callback)) = call.args.first().map(|arg| &*arg.expr) else {
            return;
        };
        self.bind_inline_parameters(
            callback.span,
            callback.params.iter(),
            [resolve
                .args
                .first()
                .and_then(|arg| self.argument_provenance(&arg.expr))],
        );
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
        // A named helper can have many call sites. Retain a parameter alias
        // only when every modeled invocation agrees, avoiding false positives
        // from joining incompatible values.
        for (callee, arguments) in &self.calls {
            let Some((scope, parameters)) = self.functions.get(callee) else {
                continue;
            };
            for (parameter, target) in parameters.iter().zip(arguments) {
                let mut projected = BTreeMap::new();
                if let Some(target) = target {
                    project_parameter_pattern(parameter, target, &mut projected);
                }
                for (name, target) in projected {
                    let entry = aliases
                        .entry((*scope, name))
                        .or_insert_with(|| Some(target.clone()));
                    if *entry != Some(target) {
                        *entry = None;
                    }
                }
            }
        }
        aliases
            .into_iter()
            .filter_map(|(key, target)| target.map(|target| (key, target)))
            .collect()
    }
}

fn project_parameter_pattern(
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
            Some(BindingProvenance::ValueAlias { target }) => Some(target.clone()),
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
            let function_constructor_alias = value_alias
                .as_deref()
                .filter(|target| *target == "Function")
                .map(|target| BindingProvenance::ValueAlias {
                    target: target.to_string(),
                });
            let returned_alias = declarator
                .init
                .as_deref()
                .and_then(|init| self.returned_object_provenance(init));
            let const_value = declarator.init.as_deref().and_then(|init| {
                self.static_object_values(init)
                    .or_else(|| self.const_provenance(init))
            });
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
            if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, module_alias.as_ref())
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
            } else if let (Pat::Ident(ident), Some(provenance)) = (&declarator.name, returned_alias)
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
                target: target.to_string(),
            });
        let provenance = self
            .module_alias_provenance(&assignment.right)
            .or(function_constructor_alias)
            .or_else(|| self.returned_object_provenance(&assignment.right))
            .or_else(|| self.const_provenance(&assignment.right))
            .or_else(|| rooted_alias.map(|target| BindingProvenance::ValueAlias { target }))
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
                        receiver_root: root.sym.to_string(),
                        receiver_span: root.span,
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
