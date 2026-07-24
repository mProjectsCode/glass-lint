//! Whitelisted inline callback binding and parameter projection.

use std::collections::{BTreeMap, HashMap};

use glass_lint_datastructures::{NameId, NameTable};
use smol_str::SmolStr;
use swc_common::Span;
use swc_ecma_ast::{CallExpr, Callee, Expr, MemberExpr, Pat};

use super::{CompactPat, compact_pat};
use crate::analysis::{
    scope::{BindingProvenance, ScopeId, ScopedName, collect::ScopeCollector},
    syntax::member_property_name,
};

impl ScopeCollector<'_> {
    /// Resolve the proven parameter aliases shared by all compatible calls to
    /// a helper. Conflicting call sites are discarded rather than merged:
    /// retaining an ambiguous alias would leak one caller's provenance into
    /// another.
    pub fn parameter_aliases(&self) -> HashMap<ScopedName, BindingProvenance> {
        let mut aliases = BTreeMap::<ScopedName, Option<BindingProvenance>>::new();
        for (caller_scope, callee_name, arguments) in &self.calls {
            let Some((scope, parameters)) = self.function_for_call(*caller_scope, *callee_name)
            else {
                continue;
            };
            for (index, parameter) in parameters.iter().enumerate() {
                let mut projected = HashMap::new();
                if *caller_scope != *scope
                    && let Some(Some(target)) = arguments.get(index)
                {
                    Self::project_parameter_pattern(&self.names, parameter, target, &mut projected);
                }
                for name in Self::parameter_binding_names(parameter) {
                    let target = projected.get(&name).cloned();
                    let Some(key) = self.scoped_name(*scope, name.as_str()) else {
                        continue;
                    };
                    let entry = aliases.entry(key).or_insert_with(|| target.clone());
                    if *entry != target {
                        *entry = None;
                    }
                }
            }
            if arguments.len() != parameters.len() {
                for parameter in parameters {
                    for name in Self::parameter_binding_names(parameter) {
                        if let Some(key) = self.scoped_name(*scope, name.as_str()) {
                            aliases.insert(key, None);
                        }
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
    fn parameter_binding_names(pattern: &CompactPat) -> Vec<SmolStr> {
        let mut names = Vec::new();
        collect_compact_binding_names(pattern, &mut names);
        names.sort();
        names.dedup();
        names
    }

    /// Project a proven object argument through a destructured parameter.
    /// Unsupported patterns intentionally contribute no bindings: callers
    /// must not infer aliases from a shape that the collector cannot prove.
    pub(super) fn project_parameter_pattern(
        names: &NameTable,
        pattern: &CompactPat,
        value: &BindingProvenance,
        output: &mut HashMap<SmolStr, BindingProvenance>,
    ) {
        match pattern {
            CompactPat::Ident(name) => {
                output.insert(name.clone(), value.clone());
            }
            CompactPat::Assign(inner) => {
                Self::project_parameter_pattern(names, inner, value, output);
            }
            CompactPat::Object(props) => {
                let BindingProvenance::StaticObjectValues(values) = value else {
                    return;
                };
                for (key, sub_pat) in props {
                    let Some(key) = names.lookup(key.as_str()) else {
                        continue;
                    };
                    let Some(target) = values.get(&key) else {
                        continue;
                    };
                    Self::project_parameter_pattern(
                        names,
                        sub_pat,
                        &BindingProvenance::ValueAlias {
                            target: target.clone(),
                        },
                        output,
                    );
                }
            }
            CompactPat::Array | CompactPat::Rest(_) | CompactPat::Other => {}
        }
    }

    fn function_for_call(
        &self,
        mut scope: ScopeId,
        name: NameId,
    ) -> Option<&(ScopeId, Vec<CompactPat>)> {
        loop {
            if let Some(function) = self.function_scopes.get(&(scope, name)) {
                return Some(function);
            }
            scope = self.scopes.get(scope.index())?.parent?;
        }
    }

    pub(super) fn function_scope_for_name(&self, name: &str) -> Option<ScopeId> {
        let name = self.names.lookup(name)?;
        self.function_for_call(self.current_scope(), name)
            .map(|(scope, _)| *scope)
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
        let mut bindings = HashMap::new();
        for (parameter, argument) in parameters.into_iter().zip(arguments) {
            if let Some(argument) = argument {
                let compact = compact_pat(parameter);
                Self::project_parameter_pattern(&self.names, &compact, &argument, &mut bindings);
            }
        }
        if !bindings.is_empty() {
            self.inline_parameters.insert(span.lo, bindings);
        }
    }

    fn record_for_each_callback(&mut self, member: &MemberExpr, call: &CallExpr) {
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
    }

    fn record_then_callback(&mut self, member: &MemberExpr, call: &CallExpr) {
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
            self.record_for_each_callback(member, call);
            return;
        }
        if method == "then" && self.is_unbound("Promise") {
            self.record_then_callback(member, call);
        }
    }
}

fn collect_compact_binding_names(pattern: &CompactPat, names: &mut Vec<SmolStr>) {
    match pattern {
        CompactPat::Ident(name) => names.push(name.clone()),
        CompactPat::Assign(inner) | CompactPat::Rest(inner) => {
            collect_compact_binding_names(inner, names);
        }
        CompactPat::Object(props) => {
            for sub in props.values() {
                collect_compact_binding_names(sub, names);
            }
        }
        CompactPat::Array | CompactPat::Other => {}
    }
}
