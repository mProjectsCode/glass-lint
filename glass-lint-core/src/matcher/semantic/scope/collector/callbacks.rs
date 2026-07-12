//! Whitelisted inline callback binding and parameter projection.

use std::collections::BTreeMap;

use swc_common::Span;
use swc_ecma_ast::{CallExpr, Callee, Expr, Pat};

use super::super::super::ast::member_prop_name;
use super::super::super::summary::project_parameter_pattern;
use super::super::BindingProvenance;
use super::AliasCollector;

impl AliasCollector {
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
}
