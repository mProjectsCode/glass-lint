use std::sync::Arc;

use glass_lint_datastructures::{NamePath, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::{
    model::value::MAX_VALUES,
    resolution::{
        ConstValue, Expr, MemberExpr, ResolvedValue, Resolver, SymbolCallProvenance, Value,
        ValueId, syntax_constant,
    },
    syntax::{TransparentTerminal, UnknownReason, effective_terminal_expr},
};

impl Resolver<'_> {
    pub(in crate::analysis) fn static_string_array_expr(&self, expr: &Expr) -> Option<Vec<String>> {
        match syntax_constant::evaluate(expr, self) {
            ConstValue::Array(values) => values
                .into_iter()
                .map(|value| value.string().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }

    pub(in crate::analysis) fn rooted_expr_chain(&mut self, expr: &Expr) -> Option<SymbolPath> {
        let terminal = effective_terminal_expr(expr)?;
        match terminal {
            TransparentTerminal::Expr(expr) => match expr {
                Expr::Ident(ident) => self
                    .resolve_ident(ident)
                    .provenance
                    .rooted_chain
                    .clone()
                    .or_else(|| {
                        ident
                            .span
                            .is_dummy()
                            .then(|| SymbolPath::from(ident.sym.as_ref()))
                    }),
                Expr::Seq(sequence) => sequence
                    .exprs
                    .last()
                    .and_then(|expr| self.rooted_expr_chain(expr)),
                Expr::TsAs(value) => self.rooted_expr_chain(&value.expr),
                Expr::TsNonNull(value) => self.rooted_expr_chain(&value.expr),
                Expr::TsSatisfies(value) => self.rooted_expr_chain(&value.expr),
                Expr::TsTypeAssertion(value) => self.rooted_expr_chain(&value.expr),
                _ => None,
            },
            TransparentTerminal::Member(member) => {
                self.resolve_member(member).provenance.rooted_chain.clone()
            }
        }
    }

    pub(in crate::analysis) fn syntactic_member_chain(
        &self,
        member: &MemberExpr,
    ) -> Option<SymbolPath> {
        let key = Self::member_key(member);
        self.cache
            .resolved_values
            .get(&key)
            .and_then(|value| value.syntactic_chain.clone())
            .or_else(|| crate::analysis::syntax::member_expression_chain(member))
    }

    pub(in crate::analysis) fn class_provenance(
        &mut self,
        expr: &Expr,
    ) -> Option<(SmolStr, SmolStr)> {
        match &self.resolve_expr(expr).call {
            SymbolCallProvenance::ModuleExport { module, export } => {
                Some((module.clone(), export.clone()))
            }
            _ => None,
        }
    }

    pub(in crate::analysis) fn unknown() -> ResolvedValue {
        Self::archive_unknown_with_reason(UnknownReason::Unresolved)
    }

    pub(in crate::analysis::resolution) fn archive_unknown_with_reason(
        reason: UnknownReason,
    ) -> ResolvedValue {
        let mut value = ResolvedValue::local(ValueId::UNKNOWN);
        Arc::make_mut(&mut value.provenance).call = SymbolCallProvenance::Unknown(reason);
        value
    }

    pub(in crate::analysis::resolution) fn interned_value(&self, id: ValueId) -> ResolvedValue {
        if id == ValueId::UNKNOWN && self.value_arena_exhausted() {
            return Self::archive_unknown_with_reason(UnknownReason::BudgetExhausted {
                limit: MAX_VALUES,
            });
        }
        ResolvedValue::local(id)
    }

    pub(in crate::analysis) fn static_string(&mut self, value: String) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::StaticString(value), None);
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_number(&mut self, value: usize) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::StaticNumber(value), None);
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_array(&mut self, values: Vec<ValueId>) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::StaticArray(values), None);
        self.interned_value(id)
    }

    pub(in crate::analysis) fn static_object_shape(
        &mut self,
        object: crate::analysis::model::value::StaticObject,
    ) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::StaticObject(object), None);
        self.interned_value(id)
    }

    fn intern_object_id(
        &mut self,
        object: crate::analysis::model::value::ResolvedObjectId,
    ) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::Object(object), None);
        self.interned_value(id)
    }

    pub(in crate::analysis) fn rooted_member(&mut self, path: NamePath) -> ResolvedValue {
        let id = self
            .values
            .intern_value_with_binding(Value::RootedMember { path }, None);
        self.interned_value(id)
    }

    pub(in crate::analysis) fn fresh_object_value(&mut self) -> ResolvedValue {
        let Some(object) = self.values.allocate_object_id() else {
            return Self::unknown();
        };
        self.intern_object_id(object)
    }

    pub(in crate::analysis) fn fresh_object_value_at(
        &mut self,
        span: swc_common::Span,
    ) -> ResolvedValue {
        let key = span.into();
        if let Some(value) = self.cache.fresh_values.get(&key).copied() {
            return ResolvedValue::local(value);
        }
        let value = self.fresh_object_value();
        self.cache.fresh_values.insert(key, value.id);
        value
    }
}
