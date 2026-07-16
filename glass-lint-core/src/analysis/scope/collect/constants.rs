//! Collector-backed adapter for bounded constant evaluation.
//!
//! Identifier lookup is restricted to the collector's use-position binding
//! state. Mutable object bindings and dynamic property names therefore become
//! unknown instead of being treated as stable constants.

use swc_ecma_ast::{Expr, Ident, MemberExpr};

use super::{super::BindingProvenance, AliasCollector};
use crate::analysis::syntax::constant::{self, ConstValue, EvalState, Lookup};

impl Lookup for AliasCollector {
    /// Resolve only constant-shaped binding provenances from the current scope.
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        match self.visible_binding(ident.sym.as_ref()) {
            Some(BindingProvenance::StaticString(value)) => ConstValue::String(value.clone()),
            Some(BindingProvenance::StaticNumber(value)) => ConstValue::NonNegativeInteger(*value),
            Some(BindingProvenance::StaticStringArray(values)) => {
                ConstValue::Array(values.iter().cloned().map(ConstValue::String).collect())
            }
            Some(BindingProvenance::StaticObjectKeys(values)) => ConstValue::Object(
                values
                    .iter()
                    .cloned()
                    .map(|key| (key, ConstValue::Unknown))
                    .collect(),
            ),
            Some(BindingProvenance::StaticObjectValues(values)) => ConstValue::Object(
                values
                    .keys()
                    .cloned()
                    .map(|key| (key, ConstValue::Unknown))
                    .collect(),
            ),
            _ => ConstValue::Unknown,
        }
    }

    /// Reject spreads from mutable static objects before recursing into them.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if let Expr::Ident(ident) = expr
            && self
                .visible_binding_scope(ident.sym.as_ref())
                .is_some_and(|scope| {
                    self.mutable_static_objects
                        .contains(&(scope, ident.sym.to_string()))
                })
        {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    /// Evaluate only statically named array/object members.
    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue {
        let Some(property) = constant::property_name_with_state(&member.prop, self, state) else {
            return ConstValue::Unknown;
        };
        match state.evaluate(&member.obj, self) {
            ConstValue::Array(values) => property
                .parse::<usize>()
                .ok()
                .and_then(|index| values.get(index).cloned())
                .unwrap_or(ConstValue::Unknown),
            ConstValue::Object(values) => values
                .get(&property)
                .cloned()
                .unwrap_or(ConstValue::Unknown),
            _ => ConstValue::Unknown,
        }
    }

    /// Delegate global recognition to lexical shadowing analysis.
    fn unshadowed_global(&self, name: &str, _span: swc_common::Span) -> bool {
        self.is_unbound(name)
    }
}
