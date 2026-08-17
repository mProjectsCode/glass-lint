use swc_ecma_ast::{Program, Stmt};

use super::*;
use crate::{parse::SourceLanguage, project::SourceFile};

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

fn typescript_expression(source: &str) -> Expr {
    let source = SourceFile::with_language(
        "module-request.ts",
        format!("{source};"),
        SourceLanguage::TypeScript,
    )
    .expect("test source should be valid");
    let parsed = crate::parse::SourceParser::new(&source)
        .expect("test expression should parse")
        .parse()
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
    let dynamic = recognize_module_call(&dynamic, &mut context, ModuleRequestPolicy::interface())
        .expect("literal dynamic import should be recognized");
    assert_eq!(dynamic.kind(), ModuleRequestKind::DynamicImport);

    let wrapped = expression("__toESM(require('sdk'))");
    let wrapped = recognize_module_expression(&wrapped, &mut context, ModuleRequestPolicy::alias())
        .expect("interop wrapper should preserve the require request");
    assert_eq!(wrapped.module(), "sdk");
    assert_eq!(wrapped.kind(), ModuleRequestKind::WrappedRequire);
}

#[test]
fn transparent_typescript_wrappers_preserve_module_request_shapes() {
    let wrapped = typescript_expression("(import('sdk') as any)");
    let request = recognize_module_expression(
        &wrapped,
        &mut TestContext::default(),
        ModuleRequestPolicy::alias_with_dynamic_import(),
    )
    .expect("typescript-wrapped dynamic import should be recognized");
    assert_eq!(request.kind(), ModuleRequestKind::DynamicImport);
    assert_eq!(request.module(), "sdk");

    let wrapped = typescript_expression("(require(name) as any)");
    assert!(
        recognize_module_expression(
            &wrapped,
            &mut TestContext::default(),
            ModuleRequestPolicy::alias_with_dynamic_import(),
        )
        .is_none()
    );
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
            ModuleRequestPolicy::interface(),
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
        recognize_module_call(&require, &mut shadowed, ModuleRequestPolicy::interface(),).is_none()
    );
}
