//! Bounded constant queries backed by lexical provenance.

use crate::analysis::scope::query::{
    BindingProvenance, ConstValue, EvalState, Expr, Ident, Lookup, MemberExpr, ScopeGraph, Span,
    constant,
};

impl ScopeGraph<'_> {
    /// Whether an identifier refers to a mutable static object binding.
    pub(in crate::analysis) fn mutable_static_object_at(&self, expr: &Expr) -> bool {
        let Expr::Ident(ident) = expr else {
            return false;
        };
        self.binding_with_scope_at(ident.sym.as_ref(), ident.span)
            .is_some_and(|(scope, _)| self.is_mutable_static_object(scope, ident.sym.as_ref()))
    }
}

impl Lookup for ScopeGraph<'_> {
    /// Convert only known static binding provenances into constant values.
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
                    .filter_map(|key| self.resolve_name_id(*key))
                    .map(|key| (key, ConstValue::Unknown))
                    .collect(),
            ),
            Some(BindingProvenance::StaticObjectValues(values)) => ConstValue::Object(
                values
                    .keys()
                    .filter_map(|key| self.resolve_name_id(*key))
                    .map(|key| (key, ConstValue::Unknown))
                    .collect(),
            ),
            _ => ConstValue::Unknown,
        }
    }

    /// Reject spreads whose source object may have been mutated.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    /// Evaluate only static array indexes and object property names.
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

    /// Delegate global checks to the position-sensitive scope query.
    fn unshadowed_global(&self, name: &str, span: Span) -> bool {
        self.unshadowed_global_at(name, span)
    }
}
