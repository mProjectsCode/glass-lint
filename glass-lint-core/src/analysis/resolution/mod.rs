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
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use glass_lint_datastructures::{
    ByteRange, NameExhausted, NameId, NamePath, NameTable, SymbolPath,
};
use smol_str::SmolStr;
#[cfg(test)]
use swc_ecma_ast::Program;
use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr};

#[cfg(test)]
use crate::Environment;
#[cfg(test)]
use crate::analysis::scope::ScopeGraph;
use crate::analysis::{
    lowering::{InvalidParserSpan, ParserSpanKey, SpanNormalizer},
    scope::{BoundArgument, FrozenScopeGraph},
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance,
        constant::{self as syntax_constant, ConstValue, EvalState, Lookup},
    },
    value::{BindingKey, Value, ValueId, ValueTable},
};

#[derive(Debug, Clone)]
/// The complete result of resolving one expression.
///
/// A resolved value carries the interned abstract value ID, all available
/// provenances (callable, member, returned-member, bound-arguments), and
/// both the syntactic and rooted chain spellings. Fields default to absent
/// or local so a new resolution path cannot accidentally inherit provenance.
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
    pub(super) bound_arguments: Option<Vec<Option<BoundArgument>>>,
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
        symbol: SmolStr,
    },
    /// Member lookup keyed by its checked source range.
    Member { range: ParserSpanKey },
}

#[derive(Debug, Default)]
/// Resolution cache and recursion guards.
struct ResolverCache {
    /// Fresh object values cached by checked source range to avoid
    /// allocating duplicate identities for the same syntactic object.
    fresh_values: BTreeMap<ParserSpanKey, ValueId>,
    /// Cached expression resolutions keyed by source position. Resolution
    /// is position-sensitive and idempotent.
    resolved_values: BTreeMap<ResolutionKey, Arc<ResolvedValue>>,
    /// Active lookups used to break recursive resolution cycles.
    resolving: BTreeSet<ResolutionKey>,
}

#[derive(Debug)]
/// Position-sensitive expression resolution.
///
/// The resolver is the single adapter from low-level scope and binding facts
/// to the versioned values consumed by matchers. Resolution is cached by
/// source position; recursive lookups are guarded. Unknown values, cycles,
/// and exhausted arena entries become local/unknown provenance.
pub(super) struct Resolver {
    /// Scope/provenance seeds from the lexical collection pass.
    scopes: FrozenScopeGraph,
    /// SWC-to-domain span conversion and validation.
    coordinates: SpanNormalizer,
    /// Interned value arena. Separated from the resolution cache so that
    /// immutable queries can borrow arena entries without deep cloning.
    values: ValueTable,
    /// Resolution cache, fresh-object map, and recursion guard.
    cache: ResolverCache,
}

impl Lookup for Resolver {
    fn ident(&self, ident: &Ident, _state: &mut EvalState) -> ConstValue {
        self.scopes.ident_value_seed(ident).constant
    }

    fn spread(&self, expr: &Expr, state: &mut EvalState) -> ConstValue {
        if self.scopes.mutable_static_object_at(expr) {
            return ConstValue::Unknown;
        }
        state.evaluate(expr, self)
    }

    fn member(&self, member: &MemberExpr, state: &mut EvalState) -> ConstValue {
        self.scopes.member(member, state)
    }

    fn unshadowed_global(&self, name: &str, span: swc_common::Span) -> bool {
        self.scopes.unshadowed_global_at(name, span)
    }
}

impl Resolver {
    /// Consume the resolver and return the name and value tables together,
    /// avoiding a clone of the name table.
    pub(in crate::analysis) fn into_parts(self) -> (NameTable, ValueTable) {
        (self.scopes.into_name_table(), self.values)
    }

    /// Convert a canonical member chain into the arena's structured value.
    /// Keeping this conversion beside `Resolver` ensures callers do not need
    /// to know how rooted values are represented internally.
    pub(super) fn rooted_value(&self, chain: &SymbolPath) -> Value {
        // `this.` is syntax context rather than part of the provider-rooted
        // identity. Canonicalize it before interning so aliases of
        // `this.app.foo` share the same frozen value as `app.foo`.
        let chain = chain.without_this_prefix();
        self.scopes
            .name_path(&chain)
            .map_or(Value::Unknown, |path| Value::RootedMember { path })
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect(program: &Program) -> Self {
        let mut environment = Environment::default();
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

    #[cfg(test)]
    pub(in crate::analysis) fn collect_with_environment(
        program: &Program,
        environment: &Environment,
        coordinates: SpanNormalizer,
    ) -> Self {
        use crate::analysis::name::MAX_NAMES;

        Self::collect_with_name_limit(program, environment, coordinates, MAX_NAMES)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect_with_name_limit(
        program: &Program,
        environment: &Environment,
        coordinates: SpanNormalizer,
        name_limit: usize,
    ) -> Self {
        let names = NameTable::with_max_entries(name_limit);
        let scopes = ScopeGraph::collect_scoped_program(program, environment, names)
            .into_parts()
            .0;
        Self::new(scopes, coordinates)
    }

    /// Build a resolver with an externally-owned name table.
    pub(super) fn new(scopes: FrozenScopeGraph, coordinates: SpanNormalizer) -> Self {
        Self {
            scopes,
            coordinates,
            values: ValueTable::default(),
            cache: ResolverCache::default(),
        }
    }

    pub(super) fn intern_name(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        self.scopes.intern_name_mut(name).ok_or_else(|| {
            self.scopes.name_exhaustion().unwrap_or(NameExhausted {
                limit: 0,
                attempted: 0,
            })
        })
    }

    pub(super) fn name_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.scopes.name_path(path)
    }

    pub(super) fn name_table_exhausted(&self) -> bool {
        self.scopes.name_table_exhausted()
    }

    pub(super) fn name_exhaustion(&self) -> Option<NameExhausted> {
        self.scopes.name_exhaustion()
    }

    #[cfg(test)]
    pub(super) fn name_snapshot(&self) -> NameTable {
        self.scopes.name_snapshot()
    }

    pub(in crate::analysis) fn normalize_span(
        &self,
        span: swc_common::Span,
    ) -> Result<ByteRange, InvalidParserSpan> {
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
        self.values.exhausted()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn value_snapshot(&self) -> ValueTable {
        self.values.clone()
    }

    pub(in crate::analysis) fn instance_member_available(&self, member: &MemberExpr) -> bool {
        self.scopes.instance_member_available_at(member)
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{NameId, NameTable};

    use super::*;
    use crate::analysis::{
        lowering::SpanNormalizer,
        scope::ScopeGraph,
        syntax::{BudgetComponent, UnknownReason},
        value::{MAX_VALUES, Value},
    };

    #[test]
    fn unknown_value_keeps_unsupported_and_exhausted_distinct() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let resolver = Resolver::new(scopes, SpanNormalizer::default());
        assert_eq!(
            resolver.call_provenance_for_value(ValueId::UNKNOWN),
            SymbolCallProvenance::Unknown(UnknownReason::Unsupported)
        );

        let mut values = ValueTable::default();
        for value in 0..MAX_VALUES {
            let _ = values.intern(Value::StaticNumber(value));
        }
        assert!(values.exhausted());
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let resolver = Resolver {
            scopes,
            coordinates: SpanNormalizer::default(),
            values,
            cache: ResolverCache::default(),
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

    #[test]
    fn const_value_follows_binding_chain_to_static_values() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let inner = resolver.values.intern(Value::StaticString("hello".into()));
        let key = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("test".into()),
        );
        let id = resolver
            .values
            .intern(Value::Binding { key, target: inner });

        let result = resolver.const_value(id);
        assert_eq!(result, ConstValue::String("hello".into()));
    }

    #[test]
    fn const_value_materializes_static_arrays_with_nested_bindings() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let one = resolver.values.intern(Value::StaticNumber(1));
        let key = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("x".into()),
        );
        let wrapped = resolver.values.intern(Value::Binding { key, target: one });
        let two = resolver.values.intern(Value::StaticNumber(2));
        let array = resolver
            .values
            .intern(Value::StaticArray(vec![wrapped, two]));

        let result = resolver.const_value(array);
        assert_eq!(
            result,
            ConstValue::Array(vec![
                ConstValue::NonNegativeInteger(1),
                ConstValue::NonNegativeInteger(2),
            ])
        );
    }

    #[test]
    fn const_value_returns_unknown_for_uninterned_id() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let resolver = Resolver::new(scopes, SpanNormalizer::default());

        let result = resolver.const_value(ValueId(u32::MAX));
        assert_eq!(result, ConstValue::Unknown);
    }

    #[test]
    fn const_value_materializes_static_object_with_mixed_values() {
        let mut names = NameTable::default();
        let key_num = names.intern("num").unwrap();
        let key_str = names.intern("str").unwrap();
        let key_arr = names.intern("arr").unwrap();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let num_id = resolver.values.intern(Value::StaticNumber(42));
        let str_id = resolver.values.intern(Value::StaticString("val".into()));
        let inner_arr = resolver.values.intern(Value::StaticArray(vec![num_id]));

        let obj_id = resolver.values.intern(Value::StaticObject(vec![
            (key_num, num_id),
            (key_str, str_id),
            (key_arr, inner_arr),
        ]));

        let result = resolver.const_value(obj_id);
        assert_eq!(
            result,
            ConstValue::Object(BTreeMap::from([
                (
                    "arr".into(),
                    ConstValue::Array(vec![ConstValue::NonNegativeInteger(42)])
                ),
                ("num".into(), ConstValue::NonNegativeInteger(42)),
                ("str".into(), ConstValue::String("val".into())),
            ]))
        );
    }

    #[test]
    fn const_value_returns_unknown_for_unknown_name_in_object() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let val_id = resolver.values.intern(Value::StaticString("v".into()));
        let bad_name = NameId(u32::MAX);
        let obj_id = resolver
            .values
            .intern(Value::StaticObject(vec![(bad_name, val_id)]));

        let result = resolver.const_value(obj_id);
        assert_eq!(result, ConstValue::Unknown);
    }

    #[test]
    fn const_value_returns_unknown_for_deeply_nested_structure() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let leaf = resolver.values.intern(Value::StaticNumber(0));
        let mut current = leaf;
        for _ in 0..31 {
            current = resolver.values.intern(Value::StaticArray(vec![current]));
        }
        let result = resolver.const_value(current);
        assert!(
            matches!(result, ConstValue::Array(_)),
            "31 nesting levels should succeed"
        );

        current = resolver.values.intern(Value::StaticArray(vec![current]));
        let result = resolver.const_value(current);
        let mut inner = &result;
        loop {
            match inner {
                ConstValue::Array(elements) if elements.len() == 1 => inner = &elements[0],
                _ => break,
            }
        }
        assert_eq!(inner, &ConstValue::Unknown);
    }

    #[test]
    fn const_value_materializes_large_flat_array() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let ids: Vec<_> = (0..100)
            .map(|i| resolver.values.intern(Value::StaticNumber(i)))
            .collect();
        let array_id = resolver.values.intern(Value::StaticArray(ids));

        let result = resolver.const_value(array_id);
        assert_eq!(
            result,
            ConstValue::Array(
                (0..100)
                    .map(ConstValue::NonNegativeInteger)
                    .collect::<Vec<_>>()
            )
        );
    }

    #[test]
    fn const_value_follows_binding_chain_through_reassignment() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let inner = resolver.values.intern(Value::StaticString("first".into()));
        let key1 = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("v1".into()),
        );
        let first = resolver.values.intern(Value::Binding {
            key: key1,
            target: inner,
        });

        let key2 = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("v2".into()),
        );
        let second = resolver.values.intern(Value::Binding {
            key: key2,
            target: first,
        });

        assert_eq!(
            resolver.const_value(first),
            ConstValue::String("first".into())
        );
        assert_eq!(
            resolver.const_value(second),
            ConstValue::String("first".into())
        );
    }

    #[test]
    fn call_provenance_follows_binding_to_global() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let inner = resolver.values.intern(Value::Global("fetch".into()));
        let key = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("test".into()),
        );
        let id = resolver
            .values
            .intern(Value::Binding { key, target: inner });

        assert_eq!(
            resolver.call_provenance_for_value(id),
            SymbolCallProvenance::Global {
                name: "fetch".into()
            }
        );
    }

    #[test]
    fn call_provenance_follows_multi_level_binding_chain() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let mut resolver = Resolver::new(scopes, SpanNormalizer::default());

        let inner = resolver.values.intern(Value::ModuleExport {
            module: "mod".into(),
            export: "fn".into(),
        });
        let key1 = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("a".into()),
        );
        let mid = resolver.values.intern(Value::Binding {
            key: key1,
            target: inner,
        });
        let key2 = crate::analysis::value::BindingKey::new(
            crate::analysis::value::BindingRoot::Global("b".into()),
        );
        let id = resolver.values.intern(Value::Binding {
            key: key2,
            target: mid,
        });

        assert_eq!(
            resolver.call_provenance_for_value(id),
            SymbolCallProvenance::ModuleExport {
                module: "mod".into(),
                export: "fn".into()
            }
        );
    }

    #[test]
    fn value_exhaustion_distinguishes_unsupported_from_budget() {
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let resolver = Resolver::new(scopes, SpanNormalizer::default());
        assert!(!resolver.value_arena_exhausted());

        let mut values = ValueTable::default();
        for value in 0..MAX_VALUES {
            let _ = values.intern(Value::StaticNumber(value));
        }
        assert!(values.exhausted());
        let names = NameTable::default();
        let scopes = ScopeGraph::create_for_test(names).freeze();
        let resolver = Resolver {
            scopes,
            coordinates: SpanNormalizer::default(),
            values,
            cache: ResolverCache::default(),
        };
        assert!(resolver.value_arena_exhausted());
    }
}
