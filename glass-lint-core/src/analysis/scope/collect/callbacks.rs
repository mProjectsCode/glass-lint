//! Whitelisted inline callback binding and parameter projection.

use std::collections::BTreeMap;

use swc_common::Span;
use swc_ecma_ast::{CallExpr, Callee, Expr, ObjectPatProp, Pat};

use super::{
    super::{
        super::syntax::{member_property_name, property_name},
        BindingProvenance, ScopeId, ScopedName,
    },
    LexicalScopeCollector,
};

impl LexicalScopeCollector {
    /// Resolve the proven parameter aliases shared by all compatible calls to
    /// a helper. Conflicting call sites are discarded rather than merged:
    /// retaining an ambiguous alias would leak one caller's provenance into
    /// another.
    pub fn parameter_aliases(&self) -> BTreeMap<ScopedName, BindingProvenance> {
        let mut aliases = BTreeMap::<ScopedName, Option<BindingProvenance>>::new();
        for (caller_scope, callee, arguments) in &self.calls {
            let Some((scope, parameters)) = self.function_for_call(*caller_scope, callee) else {
                continue;
            };
            for (index, parameter) in parameters.iter().enumerate() {
                let mut projected = BTreeMap::new();
                if *caller_scope != *scope
                    && let Some(Some(target)) = arguments.get(index)
                {
                    Self::project_parameter_pattern(parameter, target, &mut projected);
                }
                for name in Self::parameter_binding_names(parameter) {
                    let target = projected.get(&name).cloned();
                    let entry = aliases
                        .entry(ScopedName::new(*scope, name))
                        .or_insert_with(|| target.clone());
                    if *entry != target {
                        *entry = None;
                    }
                }
            }
            if arguments.len() != parameters.len() {
                for parameter in parameters {
                    for name in Self::parameter_binding_names(parameter) {
                        aliases.insert(ScopedName::new(*scope, name), None);
                    }
                }
            }
        }
        aliases
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect()
    }

    /// Return every binding introduced by a parameter pattern in stable order.
    /// Destructuring can bind the same name through several syntactic paths;
    /// sorting and deduplicating keeps the call projection deterministic.
    fn parameter_binding_names(pattern: &Pat) -> Vec<String> {
        let mut names = Vec::new();
        Self::collect_parameter_binding_names(pattern, &mut names);
        names.sort();
        names.dedup();
        names
    }

    fn collect_parameter_binding_names(pattern: &Pat, names: &mut Vec<String>) {
        match pattern {
            Pat::Ident(ident) => names.push(ident.id.sym.to_string()),
            Pat::Assign(assign) => Self::collect_parameter_binding_names(&assign.left, names),
            Pat::Object(object) => {
                for property in &object.props {
                    match property {
                        ObjectPatProp::KeyValue(property) => {
                            Self::collect_parameter_binding_names(&property.value, names);
                        }
                        ObjectPatProp::Assign(property) => names.push(property.key.sym.to_string()),
                        ObjectPatProp::Rest(property) => {
                            Self::collect_parameter_binding_names(&property.arg, names);
                        }
                    }
                }
            }
            Pat::Array(array) => {
                for element in array.elems.iter().flatten() {
                    Self::collect_parameter_binding_names(element, names);
                }
            }
            Pat::Rest(rest) => Self::collect_parameter_binding_names(&rest.arg, names),
            Pat::Expr(_) | Pat::Invalid(_) => {}
        }
    }

    /// Project a proven object argument through a destructured parameter.
    /// Unsupported patterns intentionally contribute no bindings: callers
    /// must not infer aliases from a shape that the collector cannot prove.
    pub(super) fn project_parameter_pattern(
        pattern: &Pat,
        value: &BindingProvenance,
        output: &mut BTreeMap<String, BindingProvenance>,
    ) {
        match pattern {
            Pat::Ident(ident) => {
                output.insert(ident.id.sym.to_string(), value.clone());
            }
            Pat::Assign(assign) => Self::project_parameter_pattern(&assign.left, value, output),
            Pat::Object(object) => {
                let BindingProvenance::StaticObjectValues(values) = value else {
                    return;
                };
                for property in &object.props {
                    match property {
                        ObjectPatProp::KeyValue(property) => {
                            let Some(key) = property_name(&property.key) else {
                                continue;
                            };
                            let Some(target) = values.get(&key) else {
                                continue;
                            };
                            Self::project_parameter_pattern(
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

    fn function_for_call(&self, mut scope: ScopeId, name: &str) -> Option<&(ScopeId, Vec<Pat>)> {
        loop {
            if let Some(function) = self.function_scopes.get(&(scope, name.to_string())) {
                return Some(function);
            }
            scope = self.scopes.get(scope.index())?.parent?;
        }
    }

    pub(super) fn function_scope_for_name(&self, name: &str) -> Option<ScopeId> {
        let mut scope = self.current_scope();
        loop {
            if let Some((function, _)) = self.function_scopes.get(&(scope, name.to_string())) {
                return Some(*function);
            }
            scope = self.scopes.get(scope.index())?.parent?;
        }
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
                Self::project_parameter_pattern(parameter, &argument, &mut bindings);
            }
        }
        if !bindings.is_empty() {
            self.inline_parameters.insert(span.lo, bindings);
        }
    }

    pub(super) fn record_modeled_callbacks(&mut self, call: &CallExpr) {
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
        let Some(method) = member_property_name(&member.prop) else {
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
            || member_property_name(&resolve_member.prop).as_deref() != Some("resolve")
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
}
