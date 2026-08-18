use std::sync::Arc;

use glass_lint_datastructures::{NamePath, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::{
    model::value::MAX_VALUES,
    resolution::{
        ConstValue, Expr, MemberExpr, ResolvedValue, Resolver, SymbolCallProvenance, Value,
        ValueId, syntax_constant,
    },
    syntax::{
        TransparentTerminal, UnknownReason, effective_terminal_expr, unwrap_transparent_expr,
    },
};

impl Resolver<'_> {
    pub(in crate::analysis) fn static_string_array_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match syntax_constant::evaluate(expr, self.scope_graph()) {
            ConstValue::Array(values) => values
                .into_iter()
                .map(|value| value.string().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }

    pub(in crate::analysis) fn rooted_expr_chain(&mut self, expr: &Expr) -> Option<NamePath> {
        let expr = unwrap_transparent_expr(expr)?;
        let terminal = effective_terminal_expr(expr)?;
        match terminal {
            TransparentTerminal::Expr(expr) => match expr {
                Expr::Ident(ident) => self
                    .resolve_ident(ident)
                    .provenance
                    .rooted_chain
                    .as_ref()
                    .map(|path| self.canonical_rooted_path(path))
                    .or_else(|| {
                        ident
                            .span
                            .is_dummy()
                            .then(|| self.name_path(&SymbolPath::from(ident.sym.as_ref())))
                            .flatten()
                    }),
                _ => None,
            },
            TransparentTerminal::Member(member) => self
                .resolve_member(member)
                .provenance
                .rooted_chain
                .as_ref()
                .map(|path| self.canonical_rooted_path(path)),
        }
    }

    pub(in crate::analysis) fn syntactic_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<NamePath> {
        let key = Self::member_key(member);
        self.cache
            .resolved_values
            .get(&key)
            .and_then(|value| value.provenance.syntactic_chain.clone())
            .or_else(|| {
                crate::analysis::syntax::member_expression_chain(member)
                    .and_then(|path| self.name_path(&path))
            })
    }

    pub(in crate::analysis) fn class_provenance(
        &mut self,
        expr: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        match &self.resolve_expr(expr).provenance.call {
            SymbolCallProvenance::ModuleExport { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn unknown() -> Arc<ResolvedValue> {
        Self::archive_unknown_with_reason(UnknownReason::Unresolved)
    }

    pub(in crate::analysis::resolution) fn archive_unknown_with_reason(
        reason: UnknownReason,
    ) -> Arc<ResolvedValue> {
        ResolvedValue::with_provenance(
            ValueId::UNKNOWN,
            super::ResolutionProvenance::with_call(SymbolCallProvenance::Unknown(reason)),
        )
    }

    pub(in crate::analysis::resolution) fn interned_value(
        &self,
        id: ValueId,
    ) -> Arc<ResolvedValue> {
        if id == ValueId::UNKNOWN && self.value_arena_exhausted() {
            return Self::archive_unknown_with_reason(UnknownReason::BudgetExhausted {
                limit: MAX_VALUES,
            });
        }
        ResolvedValue::local(id)
    }

    pub(in crate::analysis) fn intern_static_string(
        &mut self,
        value: String,
    ) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::StaticString(value));
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_number(&mut self, value: usize) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::StaticNumber(value));
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_array(&mut self, values: Vec<ValueId>) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::StaticArray(values));
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_object_shape(
        &mut self,
        object: crate::analysis::model::value::StaticObject,
    ) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::StaticObject(object));
        self.interned_value(id)
    }

    fn intern_object_id(
        &mut self,
        object: crate::analysis::model::value::ResolvedObjectId,
    ) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::Object(object));
        self.interned_value(id)
    }

    pub(in crate::analysis) fn rooted_member(&mut self, path: NamePath) -> Arc<ResolvedValue> {
        let id = self.values.intern_value(Value::RootedMember { path });
        self.interned_value(id)
    }

    pub(in crate::analysis) fn fresh_object_value(&mut self) -> Arc<ResolvedValue> {
        let Some(object) = self.values.allocate_object_id() else {
            return Self::unknown();
        };
        self.intern_object_id(object)
    }

    pub(in crate::analysis) fn fresh_object_value_at(
        &mut self,
        span: swc_common::Span,
    ) -> Arc<ResolvedValue> {
        let key = span.into();
        if let Some(value) = self.cache.fresh_values.get(&key).copied() {
            return ResolvedValue::local(value);
        }
        let value = self.fresh_object_value();
        self.cache.fresh_values.insert(key, value.id);
        value
    }
}
