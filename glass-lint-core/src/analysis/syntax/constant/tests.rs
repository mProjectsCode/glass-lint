use std::collections::BTreeMap;

use swc_ecma_ast::{Expr, ExprStmt, MemberExpr, Program, Stmt};

use crate::analysis::syntax::constant::{
    ConstValue, EvalState, Lookup, evaluate,
    types::{MAX_ARRAY_ITEMS, MAX_STRING_BYTES},
};

#[derive(Default)]
struct TestLookup {
    values: BTreeMap<String, ConstValue>,
    globals: bool,
}

impl Lookup for TestLookup {
    fn ident(&self, ident: &swc_ecma_ast::Ident, _state: &mut EvalState) -> ConstValue {
        self.values
            .get(ident.sym.as_ref())
            .cloned()
            .unwrap_or(ConstValue::Unknown)
    }

    fn member(&self, _member: &MemberExpr, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, _name: &str, _span: swc_common::Span) -> bool {
        self.globals
    }
}

struct RecursiveLookup {
    expression: Box<Expr>,
}

impl Lookup for RecursiveLookup {
    fn ident(&self, _ident: &swc_ecma_ast::Ident, state: &mut EvalState) -> ConstValue {
        state.evaluate(&self.expression, self)
    }

    fn member(&self, _member: &MemberExpr, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, _name: &str, _span: swc_common::Span) -> bool {
        false
    }
}

fn expression(source: &str) -> Expr {
    let parsed = crate::parse_test_source(&format!("({source});"), "constant-test.js").unwrap();
    let statements = match parsed.program {
        Program::Module(module) => module
            .body
            .into_iter()
            .filter_map(swc_ecma_ast::ModuleItem::stmt)
            .collect::<Vec<_>>(),
        Program::Script(script) => script.body,
    };
    let Stmt::Expr(ExprStmt { expr, .. }) = statements.into_iter().next().unwrap() else {
        panic!("test input did not parse as an expression statement");
    };
    *expr
}

fn eval(source: &str) -> ConstValue {
    evaluate(&expression(source), &TestLookup::default())
}

#[test]
fn preserves_typed_addition_and_uses_cooked_templates() {
    assert_eq!(eval("1 + 2"), ConstValue::NonNegativeInteger(3));
    assert_eq!(eval("'1' + 2"), ConstValue::String("12".into()));
    assert_eq!(
        eval(r"`line\n${1 + 2}`"),
        ConstValue::String("line\n3".into())
    );
    assert_eq!(eval("-1"), ConstValue::Unknown);
}

#[test]
fn evaluates_finite_arrays_objects_spreads_and_object_assign() {
    let mut lookup = TestLookup {
        globals: true,
        ..TestLookup::default()
    };
    lookup.values.insert(
        "base".into(),
        ConstValue::Object(BTreeMap::from([(
            "a".into(),
            ConstValue::String("old".into()),
        )])),
    );

    assert_eq!(
        evaluate(&expression("({ ...base, a: 'new', 2: 'two' })"), &lookup),
        ConstValue::Object(BTreeMap::from([
            ("2".into(), ConstValue::String("two".into())),
            ("a".into(), ConstValue::String("new".into())),
        ]))
    );
    assert_eq!(
        evaluate(
            &expression("Object.assign({ a: 'old' }, { a: 'new', b: 'x' })"),
            &lookup,
        )
        .object_keys(),
        Some(vec!["a".into(), "b".into()])
    );
    assert_eq!(eval("({ get x() { return 1 } })"), ConstValue::Unknown);
    assert_eq!(eval("({ method() {} })"), ConstValue::Unknown);
}

#[test]
fn fails_closed_at_container_and_string_limits() {
    let oversized_array = format!(
        "[{}]",
        std::iter::repeat_n("0", MAX_ARRAY_ITEMS + 1)
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(eval(&oversized_array), ConstValue::Unknown);

    let oversized_string = format!("'{}'", "x".repeat(MAX_STRING_BYTES + 1));
    assert_eq!(eval(&oversized_string), ConstValue::Unknown);
}

#[test]
fn rejects_shadowed_object_assign_and_unknown_spreads() {
    let lookup = TestLookup::default();
    assert_eq!(
        evaluate(&expression("Object.assign({}, { a: 1 })"), &lookup),
        ConstValue::Unknown
    );
    assert_eq!(eval("({ ...dynamic })"), ConstValue::Unknown);
}

#[test]
fn bounds_recursive_alias_lookup_work() {
    let lookup = RecursiveLookup {
        expression: Box::new(expression("alias")),
    };
    assert_eq!(evaluate(&expression("alias"), &lookup), ConstValue::Unknown);
}
