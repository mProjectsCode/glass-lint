//! Position-sensitive expression resolution.
//!
//! The lexical fact builder supplies declarations and historical assignments.
//! `Resolver` is the single adapter from those low-level facts to the versioned
//! values consumed by matchers, so callers never make matching decisions from
//! raw identifier spelling.
//!
//! Resolution is position-sensitive and cached by source range. Recursive
//! lookups are guarded; cycles, unknown values, and exhausted arena entries
//! become local/unknown provenance instead of leaking a guessed identity.

mod call;
mod constant;
mod expression;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr, Program};

use super::{
    lowering::{ParserSpanKey, SpanNormalizer},
    name::{NameTable, NameTableHandle},
    scope::ScopeGraph,
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance,
        constant::{self as syntax_constant, ConstValue, EvalState, Lookup},
    },
    value::{BindingKey, MAX_VALUES, SymbolPath, Value, ValueId, ValueTable},
};

#[derive(Debug, Clone)]
pub(super) struct ResolvedValue {
    /// The interned abstract value. `UNKNOWN` is reserved for expressions the
    /// resolver cannot describe precisely enough to match.
    pub(super) id: ValueId,
    /// Canonical rooted spelling, when the value can be followed safely.
    pub(super) rooted_chain: Option<SymbolPath>,
    /// Callable provenance used by global and module-export call matchers.
    pub(super) call: SymbolCallProvenance,
    /// Namespace provenance for member matchers, retained independently from
    /// `call` because a namespace member can also be read without being called.
    pub(super) module_member: Option<SymbolMemberProvenance>,
    /// Provenance for a member read from a function or constructor result.
    pub(super) returned_member: Option<(SymbolPath, SymbolPath)>,
    /// Arguments captured by a modeled callable value such as `bind`.
    pub(super) bound_arguments: Option<Vec<Option<super::scope::BoundArgument>>>,
    /// The source spelling before aliases are expanded.
    pub(super) syntactic_chain: Option<SymbolPath>,
}

impl ResolvedValue {
    /// Build a value with no callable or member provenance.
    ///
    /// Unknown, static, and freshly allocated object values all use this
    /// representation. Keeping the default fields here prevents a new
    /// resolution path from accidentally inheriting provenance.
    pub(super) fn local(id: ValueId) -> Self {
        Self {
            id,
            rooted_chain: None,
            call: SymbolCallProvenance::Local,
            module_member: None,
            returned_member: None,
            bound_arguments: None,
            syntactic_chain: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ResolutionKey {
    /// Identifier lookup keyed by a checked source range and spelling.
    Ident {
        range: ParserSpanKey,
        symbol: String,
    },
    /// Member lookup keyed by its checked source range.
    Member { range: ParserSpanKey },
}

#[derive(Debug, Default)]
struct ResolverState {
    /// Interned values and binding identities.
    values: ValueTable,
    /// Fresh object values reused by checked source range.
    fresh_values: BTreeMap<ParserSpanKey, ValueId>,
    /// Cached expression resolutions keyed by source position.
    resolved_values: BTreeMap<ResolutionKey, ResolvedValue>,
    /// Active lookups used to break recursive resolution cycles.
    resolving: BTreeSet<ResolutionKey>,
}

#[derive(Debug)]
pub(super) struct Resolver {
    /// Scope/provenance seeds from the lexical collection pass.
    scopes: ScopeGraph,
    names: NameTableHandle,
    coordinates: SpanNormalizer,
    /// Cohesive mutable state for value interning, caching, and recursion
    /// guards. Keeping these lifecycles together makes their borrow order
    /// explicit at the resolver boundary.
    state: RefCell<ResolverState>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            scopes: ScopeGraph::default(),
            names: NameTableHandle::new(),
            coordinates: SpanNormalizer::default(),
            state: RefCell::new(ResolverState::default()),
        }
    }
}

impl Lookup for Resolver {
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        self.const_value(self.resolve_ident(ident).id)
    }

    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.scopes.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue {
        if let Some(property) = syntax_constant::property_name_with_state(&member.prop, self, state)
        {
            return match state.evaluate(&member.obj, self) {
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
            };
        }
        ConstValue::Unknown
    }

    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool {
        self.scopes.unshadowed_global_at(name, span)
    }
}

impl Resolver {
    /// Convert a canonical member chain into the arena's structured value.
    /// Keeping this conversion beside `Resolver` ensures callers do not need
    /// to know how rooted values are represented internally.
    pub(super) fn rooted_value(chain: &SymbolPath) -> Value {
        Value::RootedMember {
            path: chain.clone(),
        }
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect(program: &Program) -> Self {
        let mut environment = crate::Environment::default();
        environment
            .add_globals([
                "app", "client", "document", "fetch", "host", "require", "vault", "window",
            ])
            .expect("test globals are valid");
        environment
            .add_global_object("window")
            .expect("test global object is valid");
        Self::collect_with_environment(program, &environment, SpanNormalizer::for_program(program))
    }

    pub(in crate::analysis) fn collect_with_environment(
        program: &Program,
        environment: &crate::Environment,
        coordinates: SpanNormalizer,
    ) -> Self {
        let names = NameTableHandle::new();
        let scopes = ScopeGraph::collect_with_environment(program, environment, names.clone());
        Self {
            scopes,
            names,
            coordinates,
            state: RefCell::new(ResolverState::default()),
        }
    }

    pub(super) fn intern_name(
        &self,
        name: &str,
    ) -> Result<super::name::NameId, super::name::NameExhausted> {
        self.names.intern(name)
    }

    pub(super) fn name_table_exhausted(&self) -> bool {
        self.names.exhausted()
    }

    #[cfg(test)]
    pub(super) fn name_snapshot(&self) -> NameTable {
        self.names.snapshot()
    }

    pub(super) fn into_name_table(self) -> NameTable {
        let Self {
            names,
            scopes,
            coordinates: _,
            state: _,
        } = self;
        drop(scopes);
        names.into_table()
    }

    pub(in crate::analysis) fn normalize_span(
        &self,
        span: swc_common::Span,
    ) -> Result<crate::ByteRange, super::lowering::InvalidParserSpan> {
        self.coordinates.normalize(span)
    }

    /// Returns the callable/value provenance visible for an exported local
    /// binding at the module boundary. The scope graph applies the same
    /// lexical and reassignment rules used at ordinary uses.
    pub(in crate::analysis) fn exported_provenance(
        &self,
        name: &str,
        span: swc_common::Span,
    ) -> SymbolCallProvenance {
        self.scopes.call_provenance(name, span)
    }

    pub(in crate::analysis) fn static_string_value(&self, id: ValueId) -> Option<String> {
        self.const_value(id).string().map(str::to_owned)
    }

    pub(in crate::analysis) fn value_arena_exhausted(&self) -> bool {
        self.state.borrow().values.exhausted()
    }

    pub(in crate::analysis) fn instance_member_available(&self, member: &MemberExpr) -> bool {
        self.scopes.instance_member_available_at(member)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{Resolver, ResolverState, SymbolCallProvenance, ValueId, ValueTable};
    use crate::analysis::{
        syntax::{BudgetComponent, UnknownReason},
        value::MAX_VALUES,
    };

    #[test]
    fn unknown_value_keeps_unsupported_and_exhausted_distinct() {
        let resolver = Resolver::default();
        assert_eq!(
            resolver.call_provenance_for_value(ValueId::UNKNOWN),
            SymbolCallProvenance::Unknown(UnknownReason::Unsupported)
        );

        let mut values = ValueTable::default();
        for value in 0..MAX_VALUES {
            let _ = values.intern(super::Value::StaticNumber(value));
        }
        assert!(values.exhausted());
        let resolver = Resolver {
            scopes: super::ScopeGraph::default(),
            names: super::NameTableHandle::new(),
            coordinates: crate::analysis::lowering::SpanNormalizer::default(),
            state: RefCell::new(ResolverState {
                values,
                ..ResolverState::default()
            }),
        };
        assert_eq!(
            resolver.call_provenance_for_value(ValueId::UNKNOWN),
            SymbolCallProvenance::Unknown(UnknownReason::BudgetExhausted {
                component: BudgetComponent::Values,
                limit: MAX_VALUES,
                observed: None,
            })
        );
    }
}
