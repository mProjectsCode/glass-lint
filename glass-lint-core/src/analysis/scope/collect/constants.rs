//! Collector-backed adapter for bounded constant evaluation.
//!
//! Identifier lookup is restricted to the collector's use-position binding
//! state. Mutable object bindings and dynamic property names therefore become
//! unknown instead of being treated as stable constants.

use smol_str::SmolStr;
use swc_ecma_ast::{Expr, Ident};

use crate::analysis::{
    scope::{collect::ScopeCollector, provenance_to_const_value},
    syntax::constant::{ConstValue, EvalState, Lookup},
};

impl Lookup for ScopeCollector<'_> {
    /// Resolve only constant-shaped binding provenances from the current scope.
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        let resolve = |key| self.names.resolve(key).map(SmolStr::new);
        self.visible_binding(ident.sym.as_ref())
            .map_or(ConstValue::Unknown, |provenance| {
                provenance_to_const_value(provenance, &resolve)
            })
    }

    /// Reject spreads from mutable static objects before recursing into them.
    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if let Expr::Ident(ident) = expr
            && self
                .visible_binding_scope(ident.sym.as_ref())
                .is_some_and(|scope| {
                    self.scoped_name(scope, ident.sym.as_ref())
                        .is_some_and(|name| self.mutable_static_objects.contains(&name))
                })
        {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    /// Delegate global recognition to lexical shadowing analysis.
    fn unshadowed_global(&self, name: &str, _span: swc_common::Span) -> bool {
        self.is_unbound(name)
    }
}
