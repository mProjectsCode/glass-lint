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

use std::sync::Arc;

use glass_lint_datastructures::{
    ByteRange, NameExhausted, NameId, NamePath, NameTable, SymbolPath,
};
use hashbrown::{HashMap, HashSet};
use smol_str::SmolStr;
#[cfg(test)]
use swc_ecma_ast::Program;
use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr};

#[cfg(test)]
use crate::Environment;
#[cfg(test)]
use crate::analysis::scope::ScopeGraph;
use crate::analysis::{
    SemanticBudget,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    fresh_values: HashMap<ParserSpanKey, ValueId>,
    /// Cached expression resolutions keyed by source position. Resolution
    /// is position-sensitive and idempotent.
    resolved_values: HashMap<ResolutionKey, Arc<ResolvedValue>>,
    /// Active lookups used to break recursive resolution cycles.
    resolving: HashSet<ResolutionKey>,
}

#[derive(Debug)]
/// Position-sensitive expression resolution.
///
/// The resolver is the single adapter from low-level scope and binding facts
/// to the versioned values consumed by matchers. Resolution is cached by
/// source position; recursive lookups are guarded. Unknown values, cycles,
/// and exhausted arena entries become local/unknown provenance.
pub(super) struct Resolver<'a> {
    /// Scope/provenance seeds from the lexical collection pass.
    scopes: FrozenScopeGraph,
    /// SWC-to-domain span conversion and validation.
    coordinates: SpanNormalizer,
    /// Interned value arena. Separated from the resolution cache so that
    /// immutable queries can borrow arena entries without deep cloning.
    values: ValueTable,
    /// Resolution cache, fresh-object map, and recursion guard.
    cache: ResolverCache,
    /// Shared semantic budget charged for each intern and resolution step.
    pub(super) budget: &'a SemanticBudget,
}

impl Lookup for Resolver<'_> {
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

impl Resolver<'_> {
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
    pub(in crate::analysis) fn collect(program: &Program, source: &str) -> Resolver<'static> {
        let mut environment = Environment::default();
        environment
            .add_globals([
                "app", "client", "document", "fetch", "host", "require", "vault", "window",
            ])
            .expect("test globals are valid");
        environment
            .add_global_object("window")
            .expect("test global object is valid");
        Self::collect_with_environment(
            program,
            &environment,
            SpanNormalizer::for_program(program, source),
        )
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect_with_environment(
        program: &Program,
        environment: &Environment,
        coordinates: SpanNormalizer,
    ) -> Resolver<'static> {
        use crate::analysis::syntax::name::MAX_NAMES;

        Self::collect_with_name_limit(program, environment, coordinates, MAX_NAMES)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect_with_name_limit(
        program: &Program,
        environment: &Environment,
        coordinates: SpanNormalizer,
        name_limit: usize,
    ) -> Resolver<'static> {
        let budget = SemanticBudget::default();
        let names = NameTable::with_max_entries(name_limit);
        let scopes = ScopeGraph::collect_scoped_program(program, environment, names, &budget)
            .into_parts()
            .0;
        Self::new(
            scopes,
            coordinates,
            Box::leak(Box::new(SemanticBudget::default())),
        )
    }

    /// Build a resolver with an externally-owned name table.
    pub(super) fn new(
        scopes: FrozenScopeGraph,
        coordinates: SpanNormalizer,
        budget: &SemanticBudget,
    ) -> Resolver<'_> {
        Resolver {
            scopes,
            coordinates,
            values: ValueTable::default(),
            cache: ResolverCache::default(),
            budget,
        }
    }

    pub(super) fn intern_name(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        self.budget.try_charge();
        if let Some(id) = self.scopes.name_id(name) {
            return Ok(id);
        }
        self.scopes.name_table_mut().intern(name)
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

    pub(in crate::analysis) fn constructed_instance_provenance(
        &self,
        ident: &Ident,
    ) -> Option<(SmolStr, SmolStr)> {
        self.scopes.constructed_instance_at(ident)
    }

    #[cfg(test)]
    pub(super) fn new_for_test(
        scopes: FrozenScopeGraph,
        coordinates: SpanNormalizer,
    ) -> Resolver<'static> {
        Self::new(
            scopes,
            coordinates,
            Box::leak(Box::new(SemanticBudget::default())),
        )
    }
}

#[cfg(test)]
mod tests;
