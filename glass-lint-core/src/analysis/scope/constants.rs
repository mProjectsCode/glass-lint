//! Collection-time adapter for constant property names.

use swc_ecma_ast::{Expr, Ident, MemberExpr};

use super::super::constant::{self, ConstValue, EvalState, Lookup};
use super::{BindingProvenance, ScopeGraph};

impl Lookup for ScopeGraph {
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        if self.has_dynamic_lookup_at(ident.span) {
            return ConstValue::Unknown;
        }
        match self.binding_at(ident.sym.as_ref(), ident.span) {
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

    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

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

    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool {
        self.unshadowed_global_at(name, span)
    }
}
