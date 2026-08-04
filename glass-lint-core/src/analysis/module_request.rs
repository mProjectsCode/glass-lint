//! Shared recognition of provider-neutral module-loading expressions.
//!
//! Syntax determines the request shape, while each semantic phase supplies
//! the position-sensitive checks for `require`, interop wrappers, and static
//! strings. Keeping those checks behind one context prevents the resolver,
//! scope collector, and fact interface from drifting apart.

use swc_common::{Span, Spanned};
use swc_ecma_ast::{Callee, Expr, Ident, Lit};

use crate::analysis::module::COMMONJS_REQUIRE;

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
pub(super) struct ModuleRequestPolicy {
    pub(super) allow_dynamic_import: bool,
    pub(super) allow_wrappers: bool,
    pub(super) require_one_argument: bool,
}

impl ModuleRequestPolicy {
    pub(super) const fn interface() -> Self {
        Self {
            allow_dynamic_import: true,
            allow_wrappers: false,
            require_one_argument: true,
        }
    }

    pub(super) const fn direct_require() -> Self {
        Self {
            allow_dynamic_import: false,
            allow_wrappers: false,
            require_one_argument: false,
        }
    }

    pub(super) const fn alias() -> Self {
        Self {
            allow_dynamic_import: false,
            allow_wrappers: true,
            require_one_argument: false,
        }
    }

    pub(super) const fn alias_with_dynamic_import() -> Self {
        Self {
            allow_dynamic_import: true,
            allow_wrappers: true,
            require_one_argument: false,
        }
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
        if !policy.allow_dynamic_import || !matches!(call.callee, Callee::Import(_)) {
            return None;
        }
        return dynamic_import(call, context);
    };

    let Expr::Ident(ident) = &**callee else {
        return None;
    };
    if policy.allow_wrappers
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
    if policy.require_one_argument && call.args.len() != 1 {
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
mod tests {
    use swc_ecma_ast::{Program, Stmt};

    use super::*;

    #[derive(Default)]
    struct TestContext {
        require_bound: bool,
        wrapper_bound: bool,
    }

    impl ModuleRequestContext for TestContext {
        fn is_unshadowed_require(&mut self, ident: &Ident) -> bool {
            ident.sym == COMMONJS_REQUIRE && !self.require_bound
        }

        fn is_unshadowed_wrapper(&mut self, ident: &Ident) -> bool {
            is_interop_wrapper(ident.sym.as_ref()) && !self.wrapper_bound
        }

        fn static_string(&mut self, expr: &Expr) -> Option<String> {
            let Expr::Lit(Lit::Str(value)) = expr else {
                return None;
            };
            Some(value.value.to_string_lossy().to_string())
        }
    }

    fn expression(source: &str) -> Expr {
        let parsed = crate::parse_test_source(&format!("{source};"), "module-request.js")
            .expect("test expression should parse");
        let Program::Script(script) = parsed.program else {
            panic!("test expression should parse as a script");
        };
        let Stmt::Expr(statement) = script.body.into_iter().next().unwrap() else {
            panic!("test source should contain one expression");
        };
        *statement.expr
    }

    #[test]
    fn direct_require_is_shared_by_all_callers() {
        let expr = expression("require('sdk')");
        let Expr::Call(call) = expr else {
            panic!("expected call expression");
        };
        let request = recognize_module_call(
            &call,
            &mut TestContext::default(),
            ModuleRequestPolicy::interface(),
        )
        .expect("direct require should be recognized");
        assert_eq!(request.module(), "sdk");
        assert_eq!(request.kind(), ModuleRequestKind::Require);
    }

    #[test]
    fn dynamic_import_and_interop_require_use_explicit_kinds() {
        let Expr::Call(dynamic) = expression("import('sdk')") else {
            panic!("expected dynamic import call");
        };
        let mut context = TestContext::default();
        let dynamic =
            recognize_module_call(&dynamic, &mut context, ModuleRequestPolicy::interface())
                .expect("literal dynamic import should be recognized");
        assert_eq!(dynamic.kind(), ModuleRequestKind::DynamicImport);

        let wrapped = expression("__toESM(require('sdk'))");
        let wrapped =
            recognize_module_expression(&wrapped, &mut context, ModuleRequestPolicy::alias())
                .expect("interop wrapper should preserve the require request");
        assert_eq!(wrapped.module(), "sdk");
        assert_eq!(wrapped.kind(), ModuleRequestKind::WrappedRequire);
    }

    #[test]
    fn shadowed_or_dynamic_module_names_fail_closed() {
        let Expr::Call(require) = expression("require(name)") else {
            panic!("expected require call");
        };
        assert!(
            recognize_module_call(
                &require,
                &mut TestContext::default(),
                ModuleRequestPolicy::direct_require(),
            )
            .is_none()
        );

        let Expr::Call(import) = expression("import(name)") else {
            panic!("expected import call");
        };
        assert!(
            recognize_module_call(
                &import,
                &mut TestContext::default(),
                ModuleRequestPolicy::interface(),
            )
            .is_none()
        );

        let mut shadowed = TestContext {
            require_bound: true,
            ..TestContext::default()
        };
        let Expr::Call(require) = expression("require('sdk')") else {
            panic!("expected require call");
        };
        assert!(
            recognize_module_call(
                &require,
                &mut shadowed,
                ModuleRequestPolicy::direct_require(),
            )
            .is_none()
        );
    }
}
