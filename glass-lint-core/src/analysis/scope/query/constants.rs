//! Bounded constant queries backed by lexical provenance.

use crate::analysis::scope::query::{
    ConstValue, EvalState, Expr, FrozenScopeGraph, Ident, Lookup, Span,
};

impl FrozenScopeGraph {
    /// Whether an identifier refers to a mutable static object binding.
    pub(in crate::analysis) fn mutable_static_object_at(&self, expr: &Expr) -> bool {
        let Expr::Ident(ident) = expr else {
            return false;
        };
        self.binding_with_scope_at(ident.sym.as_ref(), ident.span)
            .is_some_and(|(scope, _)| {
                let name = self.name_id(ident.sym.as_ref());
                name.is_some_and(|name| self.is_mutable_static_object(scope, name))
            })
    }
}

impl Lookup for FrozenScopeGraph {
    /// Convert only known static binding provenances into constant values.
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        self.ident_value_seed(ident).constant
    }

    /// Reject spreads whose source object may have been mutated.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    /// Delegate global checks to the position-sensitive scope query.
    fn unshadowed_global(&self, name: &str, span: Span) -> bool {
        self.unshadowed_global_at(name, span)
    }
}
