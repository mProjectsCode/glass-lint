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

use super::{
    scope::ScopeGraph,
    syntax::constant::{self as syntax_constant, ConstValue, EvalState, Lookup},
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
    value::{BindingKey, Value, ValueArena, ValueId},
};
use swc_ecma_ast::{CallExpr, Callee, Expr, Ident, Lit, MemberExpr, Program};

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
    pub(super) returned_member: Option<(String, String)>,
    pub(super) bound_arguments: Option<Vec<Option<super::scope::BoundArgument>>>,
    pub(super) syntactic_chain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ResolutionKey {
    Ident { lo: u32, hi: u32, symbol: String },
    Member { lo: u32, hi: u32 },
}

#[derive(Debug)]
pub(super) struct Resolver {
    scopes: ScopeGraph,
    values: RefCell<ValueArena>,
    fresh_values: RefCell<BTreeMap<(u32, u32), ValueId>>,
    resolved_values: RefCell<BTreeMap<ResolutionKey, ResolvedValue>>,
    resolving: RefCell<BTreeSet<ResolutionKey>>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            scopes: ScopeGraph::default(),
            values: RefCell::new(ValueArena::default()),
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

fn rooted_value(chain: &str) -> Value {
    let mut segments = chain.split('.');
    let root = segments.next().unwrap_or_default().to_string();
    Value::RootedMember {
        root,
        path: segments.map(str::to_string).collect(),
    }
}

impl Resolver {
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
            values: RefCell::new(ValueArena::default()),
            fresh_values: RefCell::new(BTreeMap::new()),
            resolved_values: RefCell::new(BTreeMap::new()),
            resolving: RefCell::new(BTreeSet::new()),
        }
    }
}
