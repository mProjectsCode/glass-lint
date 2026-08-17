//! Shared recognition of provider-neutral module-loading expressions.
//!
//! Syntax determines the request shape, while each semantic phase supplies
//! the position-sensitive checks for `require`, interop wrappers, and static
//! strings. Keeping those checks behind one context prevents the resolver,
//! scope collector, and fact interface from drifting apart.

use swc_common::{Span, Spanned};
use swc_ecma_ast::{Callee, Expr, Ident, Lit};

use crate::analysis::{model::module::COMMONJS_REQUIRE, syntax::is_dynamic_import};

const INTEROP_WRAPPERS: &[&str] = &[
    "__toESM",
    "__importStar",
    "__importDefault",
    "_interopRequireWildcard",
    "_interopRequireDefault",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModuleRequestKind {
    DynamicImport,
    Require,
    WrappedRequire,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ModuleRequestPolicy {
    Interface,
    Alias,
    AliasWithDynamicImport,
}

impl ModuleRequestPolicy {
    pub(super) const fn interface() -> Self {
        Self::Interface
    }

    pub(super) const fn alias() -> Self {
        Self::Alias
    }

    pub(super) const fn alias_with_dynamic_import() -> Self {
        Self::AliasWithDynamicImport
    }

    const fn allows_dynamic_import(self) -> bool {
        matches!(self, Self::Interface | Self::AliasWithDynamicImport)
    }

    const fn allows_interop_wrapper(self) -> bool {
        matches!(self, Self::Alias | Self::AliasWithDynamicImport)
    }

    const fn requires_single_require_argument(self) -> bool {
        matches!(self, Self::Interface)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RecognizedModuleRequest {
    module: String,
    kind: ModuleRequestKind,
    specifier_span: Span,
}

impl RecognizedModuleRequest {
    pub(super) fn module(&self) -> &str {
        &self.module
    }

    pub(super) const fn kind(&self) -> ModuleRequestKind {
        self.kind
    }

    pub(super) const fn specifier_span(&self) -> Span {
        self.specifier_span
    }

    fn wrapped(mut self) -> Self {
        self.kind = ModuleRequestKind::WrappedRequire;
        self
    }
}

pub(super) trait ModuleRequestContext {
    fn is_unshadowed_require(&mut self, ident: &Ident) -> bool;
    fn is_unshadowed_wrapper(&mut self, ident: &Ident) -> bool;
    fn static_string(&mut self, expr: &Expr) -> Option<String>;
}

pub(super) fn recognize_module_call<C: ModuleRequestContext + ?Sized>(
    call: &swc_ecma_ast::CallExpr,
    context: &mut C,
    policy: ModuleRequestPolicy,
) -> Option<RecognizedModuleRequest> {
    let Callee::Expr(callee) = &call.callee else {
        if !policy.allows_dynamic_import() || !is_dynamic_import(&call.callee) {
            return None;
        }
        return dynamic_import(call, context);
    };

    let Expr::Ident(ident) = &**callee else {
        return None;
    };
    if policy.allows_interop_wrapper()
        && is_interop_wrapper(ident.sym.as_ref())
        && context.is_unshadowed_wrapper(ident)
    {
        let argument = call.args.first()?;
        if argument.spread.is_some() {
            return None;
        }
        let request = recognize_module_expression(&argument.expr, context, policy)?;
        return matches!(
            request.kind(),
            ModuleRequestKind::Require | ModuleRequestKind::WrappedRequire
        )
        .then(|| request.wrapped());
    }
    if ident.sym != COMMONJS_REQUIRE || !context.is_unshadowed_require(ident) {
        return None;
    }
    if policy.requires_single_require_argument() && call.args.len() != 1 {
        return None;
    }
    let argument = call.args.first()?;
    if argument.spread.is_some() {
        return None;
    }
    let Expr::Lit(Lit::Str(specifier)) = &*argument.expr else {
        return None;
    };
    Some(RecognizedModuleRequest {
        module: specifier.value.to_string_lossy().to_string(),
        kind: ModuleRequestKind::Require,
        specifier_span: argument.expr.span(),
    })
}

pub(super) fn recognize_dynamic_import_call<C: ModuleRequestContext + ?Sized>(
    call: &swc_ecma_ast::CallExpr,
    context: &mut C,
) -> Option<RecognizedModuleRequest> {
    is_dynamic_import(&call.callee).then(|| dynamic_import(call, context))?
}

pub(super) fn recognize_module_expression<C: ModuleRequestContext + ?Sized>(
    expr: &Expr,
    context: &mut C,
    policy: ModuleRequestPolicy,
) -> Option<RecognizedModuleRequest> {
    match expr {
        Expr::Call(call) => recognize_module_call(call, context, policy),
        Expr::Member(member) => recognize_module_expression(&member.obj, context, policy),
        Expr::Paren(paren) => recognize_module_expression(&paren.expr, context, policy),
        Expr::Seq(sequence) => sequence
            .exprs
            .last()
            .and_then(|expr| recognize_module_expression(expr, context, policy)),
        _ => None,
    }
}

fn dynamic_import<C: ModuleRequestContext + ?Sized>(
    call: &swc_ecma_ast::CallExpr,
    context: &mut C,
) -> Option<RecognizedModuleRequest> {
    let argument = call.args.first()?;
    if argument.spread.is_some() {
        return None;
    }
    Some(RecognizedModuleRequest {
        module: context.static_string(&argument.expr)?,
        kind: ModuleRequestKind::DynamicImport,
        specifier_span: argument.expr.span(),
    })
}

pub(super) fn is_interop_wrapper(name: &str) -> bool {
    INTEROP_WRAPPERS.contains(&name)
}

#[cfg(test)]
mod tests;
