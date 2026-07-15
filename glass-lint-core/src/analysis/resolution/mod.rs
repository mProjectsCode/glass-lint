//! Position-sensitive expression resolution.
//!
//! The lexical fact builder supplies declarations and historical assignments.
//! `Resolver` is the single adapter from those low-level facts to the versioned
//! values consumed by matchers, so callers never make matching decisions from
//! raw identifier spelling.

mod call;
mod constant;
mod expression;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr, Program};

use super::{
    scope::ScopeGraph,
    syntax::{
        SymbolCallProvenance, SymbolMemberProvenance,
        constant::{self as syntax_constant, ConstValue, EvalState, Lookup},
    },
    value::{BindingKey, Value, ValueId, ValueTable},
};

#[derive(Debug, Clone)]
pub(super) struct ResolvedValue {
    /// The interned abstract value. `UNKNOWN` is reserved for expressions the
    /// resolver cannot describe precisely enough to match.
    pub(super) id: ValueId,
    /// Canonical rooted spelling, when the value can be followed safely.
    pub(super) rooted_chain: Option<String>,
    /// Callable provenance used by global and module-export call matchers.
    pub(super) call: SymbolCallProvenance,
    /// Namespace provenance for member matchers, retained independently from
    /// `call` because a namespace member can also be read without being called.
    pub(super) module_member: Option<SymbolMemberProvenance>,
    /// Provenance for a member read from a function or constructor result.
    pub(super) returned_member: Option<(String, String)>,
    /// Arguments captured by a modeled callable value such as `bind`.
    pub(super) bound_arguments: Option<Vec<Option<super::scope::BoundArgument>>>,
    /// The source spelling before aliases are expanded.
    pub(super) syntactic_chain: Option<String>,
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
    Ident { lo: u32, hi: u32, symbol: String },
    Member { lo: u32, hi: u32 },
}

#[derive(Debug)]
pub(super) struct Resolver {
    scopes: ScopeGraph,
    values: RefCell<ValueTable>,
    fresh_values: RefCell<BTreeMap<(u32, u32), ValueId>>,
    resolved_values: RefCell<BTreeMap<ResolutionKey, ResolvedValue>>,
    resolving: RefCell<BTreeSet<ResolutionKey>>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            scopes: ScopeGraph::default(),
            values: RefCell::new(ValueTable::default()),
            fresh_values: RefCell::new(BTreeMap::new()),
            resolved_values: RefCell::new(BTreeMap::new()),
            resolving: RefCell::new(BTreeSet::new()),
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
    pub(super) fn rooted_value(chain: &str) -> Value {
        let mut segments = chain.split('.');
        let root = segments.next().unwrap_or_default().to_string();
        Value::RootedMember {
            root,
            path: segments.map(str::to_string).collect(),
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
        Self::collect_with_environment(program, &environment)
    }

    pub(in crate::analysis) fn collect_with_environment(
        program: &Program,
        environment: &crate::Environment,
    ) -> Self {
        let scopes = ScopeGraph::collect_with_environment(program, environment);
        Self {
            scopes,
            values: RefCell::new(ValueTable::default()),
            fresh_values: RefCell::new(BTreeMap::new()),
            resolved_values: RefCell::new(BTreeMap::new()),
            resolving: RefCell::new(BTreeSet::new()),
        }
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
}
