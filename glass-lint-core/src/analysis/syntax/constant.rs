//! Small, fail-closed constant evaluator shared by semantic collection.
//!
//! This is intentionally not JavaScript evaluation.  It recognizes only the
//! finite values needed by matchers and carries an explicit bound for every
//! recursive/container operation.  Unknown values never get coerced into a
//! string merely because a property name or argument happens to be needed.

use std::collections::BTreeMap;

use swc_common::Spanned;
use swc_ecma_ast::{
    BinExpr, Expr, Ident, Lit, MemberExpr, MemberProp, ObjectLit, Prop, PropName, PropOrSpread,
};

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 4_096;
const MAX_LOOKUPS: usize = 512;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_ARRAY_ITEMS: usize = 256;
const MAX_OBJECT_KEYS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) enum ConstValue {
    Unknown,
    String(String),
    NonNegativeInteger(usize),
    Array(Vec<ConstValue>),
    Object(BTreeMap<String, ConstValue>),
}

impl ConstValue {
    /// Construct a string only when it fits the evaluator's global bound.
    /// Keeping the limit at the value boundary prevents one evaluation path
    /// from accidentally returning an oversized string.
    fn bounded_string(value: String) -> Self {
        if value.len() <= MAX_STRING_BYTES {
            Self::String(value)
        } else {
            Self::Unknown
        }
    }

    pub(in crate::analysis) fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(in crate::analysis) fn property_key(&self) -> Option<String> {
        match self {
            Self::String(value) => Some(value.clone()),
            Self::NonNegativeInteger(value) => Some(value.to_string()),
            _ => None,
        }
    }

    pub(in crate::analysis) fn object_keys(&self) -> Option<Vec<String>> {
        match self {
            Self::Object(values) => Some(values.keys().cloned().collect()),
            _ => None,
        }
    }
}

pub(in crate::analysis) trait Lookup {
    fn ident(&self, ident: &Ident, state: &mut EvalState) -> ConstValue;
    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue;
    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool;

    /// Spreading a mutable object is intentionally weaker than passing that
    /// object directly: a later mutation or reassignment can change the
    /// copied shape before the use site.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue
    where
        Self: Sized,
    {
        state.evaluate(expr, self)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct NoLookup;

impl Lookup for NoLookup {
    fn ident(&self, _ident: &Ident, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn member(&self, _member: &MemberExpr, _state: &mut EvalState) -> ConstValue {
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, _name: &str, _span: swc_common::Span) -> bool {
        false
    }
}

pub(in crate::analysis) fn evaluate(expr: &Expr, lookup: &impl Lookup) -> ConstValue {
    let mut state = EvalState::default();
    state.evaluate(expr, lookup)
}

pub(in crate::analysis) fn property_name(
    prop: &MemberProp,
    lookup: &impl Lookup,
) -> Option<String> {
    let mut state = EvalState::default();
    property_name_with_state(prop, lookup, &mut state)
}

pub(in crate::analysis) fn property_name_with_state(
    prop: &MemberProp,
    lookup: &impl Lookup,
    state: &mut EvalState,
) -> Option<String> {
    state.member_property_name(prop, lookup)
}

#[derive(Default)]
pub(in crate::analysis) struct EvalState {
    depth: usize,
    nodes: usize,
    lookups: usize,
}

impl EvalState {
    pub(in crate::analysis) fn evaluate(
        &mut self,
        expr: &Expr,
        lookup: &impl Lookup,
    ) -> ConstValue {
        if self.depth >= MAX_DEPTH || self.nodes >= MAX_NODES {
            return ConstValue::Unknown;
        }
        self.nodes += 1;
        self.depth += 1;
        let value = self.evaluate_inner(expr, lookup);
        self.depth -= 1;
        value
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn evaluate_inner(&mut self, expr: &Expr, lookup: &impl Lookup) -> ConstValue {
        match expr {
            Expr::Lit(Lit::Str(value)) => {
                ConstValue::bounded_string(value.value.to_string_lossy().to_string())
            }
            Expr::Lit(Lit::Num(value))
                if value.value.is_finite()
                    && value.value >= 0.0
                    && value.value.fract() == 0.0
                    && value.value <= (usize::MAX as u64) as f64 =>
            {
                ConstValue::NonNegativeInteger(value.value as usize)
            }
            Expr::Ident(ident) => self.lookup_ident(lookup, ident),
            Expr::Member(member) => self.lookup_member(lookup, member),
            Expr::Paren(paren) => self.evaluate(&paren.expr, lookup),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .map_or(ConstValue::Unknown, |expr| self.evaluate(expr, lookup)),
            Expr::Assign(assign) => self.evaluate(&assign.right, lookup),
            Expr::Bin(binary) if binary.op == swc_ecma_ast::BinaryOp::Add => {
                self.evaluate_add(binary, lookup)
            }
            Expr::Tpl(template) => {
                let mut output = String::new();
                for (index, quasi) in template.quasis.iter().enumerate() {
                    let cooked = quasi.cooked.as_ref().map_or_else(
                        || quasi.raw.to_string(),
                        |value| value.to_string_lossy().to_string(),
                    );
                    if !Self::append_bounded(&mut output, &cooked) {
                        return ConstValue::Unknown;
                    }
                    if let Some(expression) = template.exprs.get(index) {
                        let Some(value) = self.evaluate(expression, lookup).to_property_string()
                        else {
                            return ConstValue::Unknown;
                        };
                        if !Self::append_bounded(&mut output, &value) {
                            return ConstValue::Unknown;
                        }
                    }
                }
                ConstValue::String(output)
            }
            Expr::Array(array) if array.elems.len() <= MAX_ARRAY_ITEMS => {
                let mut values = Vec::with_capacity(array.elems.len());
                for element in &array.elems {
                    let Some(element) = element else {
                        return ConstValue::Unknown;
                    };
                    values.push(self.evaluate(&element.expr, lookup));
                }
                ConstValue::Array(values)
            }
            Expr::Object(object) => self.evaluate_object(object, lookup),
            Expr::Call(call) => self.evaluate_object_assign(call, lookup),
            Expr::TsAs(value) => self.evaluate(&value.expr, lookup),
            Expr::TsNonNull(value) => self.evaluate(&value.expr, lookup),
            Expr::TsSatisfies(value) => self.evaluate(&value.expr, lookup),
            Expr::TsTypeAssertion(value) => self.evaluate(&value.expr, lookup),
            _ => ConstValue::Unknown,
        }
    }

    fn evaluate_add(&mut self, binary: &BinExpr, lookup: &impl Lookup) -> ConstValue {
        let left = self.evaluate(&binary.left, lookup);
        let right = self.evaluate(&binary.right, lookup);
        match (&left, &right) {
            (ConstValue::NonNegativeInteger(left), ConstValue::NonNegativeInteger(right)) => left
                .checked_add(*right)
                .map_or(ConstValue::Unknown, ConstValue::NonNegativeInteger),
            (ConstValue::String(_), _) | (_, ConstValue::String(_)) => {
                let Some(left) = left.to_property_string() else {
                    return ConstValue::Unknown;
                };
                let Some(right) = right.to_property_string() else {
                    return ConstValue::Unknown;
                };
                let mut value = left;
                if !Self::append_bounded(&mut value, &right) {
                    return ConstValue::Unknown;
                }
                ConstValue::String(value)
            }
            _ => ConstValue::Unknown,
        }
    }

    fn evaluate_object(&mut self, object: &ObjectLit, lookup: &impl Lookup) -> ConstValue {
        if object.props.len() > MAX_OBJECT_KEYS {
            return ConstValue::Unknown;
        }
        let mut values = BTreeMap::new();
        for property in &object.props {
            match property {
                PropOrSpread::Spread(spread) => {
                    let ConstValue::Object(spread_values) = lookup.spread(&spread.expr, self)
                    else {
                        return ConstValue::Unknown;
                    };
                    if values.len().saturating_add(spread_values.len()) > MAX_OBJECT_KEYS {
                        return ConstValue::Unknown;
                    }
                    values.extend(spread_values);
                }
                PropOrSpread::Prop(property) => {
                    let (key, value) = match &**property {
                        Prop::Shorthand(ident) => (
                            ident.sym.to_string(),
                            self.evaluate(&Expr::Ident(ident.clone()), lookup),
                        ),
                        Prop::KeyValue(property) => {
                            let Some(key) = self.property_name(&property.key, lookup) else {
                                return ConstValue::Unknown;
                            };
                            (key, self.evaluate(&property.value, lookup))
                        }
                        // A getter, setter, or method is executable behavior,
                        // not a static object shape.
                        _ => return ConstValue::Unknown,
                    };
                    values.insert(key, value);
                }
            }
        }
        ConstValue::Object(values)
    }

    fn evaluate_object_assign(
        &mut self,
        call: &swc_ecma_ast::CallExpr,
        lookup: &impl Lookup,
    ) -> ConstValue {
        let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
            return ConstValue::Unknown;
        };
        let Expr::Member(member) = &**callee else {
            return ConstValue::Unknown;
        };
        if property_name_with_state(&member.prop, lookup, self).as_deref() != Some("assign")
            || !matches!(&*member.obj, Expr::Ident(ident) if ident.sym == *"Object")
            || !lookup.unshadowed_global("Object", member.obj.span())
            || call.args.is_empty()
        {
            return ConstValue::Unknown;
        }
        let mut values = BTreeMap::new();
        for argument in &call.args {
            let ConstValue::Object(argument_values) = self.evaluate(&argument.expr, lookup) else {
                return ConstValue::Unknown;
            };
            if values.len().saturating_add(argument_values.len()) > MAX_OBJECT_KEYS {
                return ConstValue::Unknown;
            }
            values.extend(argument_values);
        }
        ConstValue::Object(values)
    }

    fn lookup_ident(&mut self, lookup: &impl Lookup, ident: &Ident) -> ConstValue {
        if !self.consume_lookup() {
            return ConstValue::Unknown;
        }
        lookup.ident(ident, self)
    }

    fn lookup_member(&mut self, lookup: &impl Lookup, member: &MemberExpr) -> ConstValue {
        if !self.consume_lookup() {
            return ConstValue::Unknown;
        }
        lookup.member(member, self)
    }

    fn consume_lookup(&mut self) -> bool {
        if self.lookups >= MAX_LOOKUPS {
            return false;
        }
        self.lookups += 1;
        true
    }

    /// Resolve a property name using the same bounded evaluator state as its
    /// surrounding expression. Computed keys therefore consume depth, node,
    /// and lookup budget instead of silently starting a second evaluation.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn property_name(&mut self, prop: &PropName, lookup: &impl Lookup) -> Option<String> {
        match prop {
            PropName::Ident(ident) => Some(ident.sym.to_string()),
            PropName::Str(value) => {
                ConstValue::bounded_string(value.value.to_string_lossy().to_string()).property_key()
            }
            PropName::Num(value)
                if value.value.is_finite()
                    && value.value >= 0.0
                    && value.value.fract() == 0.0
                    && value.value <= usize::MAX as f64 =>
            {
                Some((value.value as usize).to_string())
            }
            PropName::Num(_) | PropName::BigInt(_) => None,
            PropName::Computed(computed) => self.evaluate(&computed.expr, lookup).property_key(),
        }
    }

    fn member_property_name(&mut self, prop: &MemberProp, lookup: &impl Lookup) -> Option<String> {
        match prop {
            MemberProp::Ident(ident) => Some(ident.sym.to_string()),
            MemberProp::PrivateName(name) => Some(format!("#{}", name.name)),
            MemberProp::Computed(computed) => self.evaluate(&computed.expr, lookup).property_key(),
        }
    }

    fn append_bounded(output: &mut String, value: &str) -> bool {
        if output.len().saturating_add(value.len()) > MAX_STRING_BYTES {
            return false;
        }
        output.push_str(value);
        true
    }
}

impl ConstValue {
    fn to_property_string(&self) -> Option<String> {
        match self {
            Self::String(value) => Some(value.clone()),
            Self::NonNegativeInteger(value) => Some(value.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use swc_ecma_ast::{Expr, ExprStmt, MemberExpr, Program, Stmt};

    use super::{ConstValue, Lookup, MAX_ARRAY_ITEMS, MAX_STRING_BYTES, evaluate};

    #[derive(Default)]
    struct TestLookup {
        values: BTreeMap<String, ConstValue>,
        globals: bool,
    }

    impl Lookup for TestLookup {
        fn ident(&self, ident: &swc_ecma_ast::Ident, _state: &mut super::EvalState) -> ConstValue {
            self.values
                .get(ident.sym.as_ref())
                .cloned()
                .unwrap_or(ConstValue::Unknown)
        }

        fn member(&self, _member: &MemberExpr, _state: &mut super::EvalState) -> ConstValue {
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
        fn ident(&self, _ident: &swc_ecma_ast::Ident, state: &mut super::EvalState) -> ConstValue {
            state.evaluate(&self.expression, self)
        }

        fn member(&self, _member: &MemberExpr, _state: &mut super::EvalState) -> ConstValue {
            ConstValue::Unknown
        }

        fn unshadowed_global(&self, _name: &str, _span: swc_common::Span) -> bool {
            false
        }
    }

    fn expression(source: &str) -> Expr {
        let parsed = crate::parse(&format!("({source});"), "constant-test.js").unwrap();
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
}
