//! Declaration-owned query semantics and typed logical query algebra.
//!
//! Types in this module are provider-neutral and validated at construction.
//! They represent authored intent without exposing compiler IR. The compiler
//! layer lowers these into physical execution plans.
//!
//! The primary authoring API consists of constructor methods on [`EventQuery`].
//! Rule authors create event queries and pass them directly to
//! [`crate::api::rule::RuleBuilder::query`] (via the [`IntoQueryDecl`] adapter)
//! or convert them with [`EventQuery::into_query`] when composing alternatives
//! or conjunctions.
//!
//! [`EventQuery::call_global`] and the other identity/event combinators replace
//! the former [`QueryDecl`] builder.
use std::{collections::BTreeSet, fmt};

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ModuleSpecifierPattern,
        query::{
            lifecycle::{LifecycleCompletion, LifecycleCondition},
            value::{ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ValueMatcher},
        },
    },
};

pub(crate) mod expression;
pub(crate) mod lifecycle;
pub(crate) mod limits;
pub(crate) mod value;
pub(crate) use expression::QueryExprKind;
pub use expression::{AllExpr, AnyExpr, QueryExpr};

pub(crate) mod event;
pub use event::{EventSpec, IdentitySpec};
pub(crate) mod error;
pub use error::{QueryBuildError, QueryDiagnostic};

// ── Typed logical query algebra ───────────────────────────────────────

/// Dense compiler variable ID assigned during query construction.
///
/// Variables are the main extensibility mechanism. Each variable has a semantic
/// type (event, object, identity, value) enforced by the predicates that bind
/// or constrain it. Variable names are authoring concerns and should not remain
/// in physical plans; runtime slots use dense validated IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarId(u32);

impl VarId {
    /// Create a variable ID from a raw index.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Return the raw index.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

/// Compiler-inferred variable type for type-checking passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[allow(dead_code)]
pub(crate) enum VarType {
    Event,
    CallEvent,
    MemberEvent,
    Object,
}

/// Binding atom: selects one event variable at a call/member/import site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct EventSelection {
    pub(crate) bind: VarId,
}

/// A predicate that constrains an already-bound variable.
///
/// `ReturnedObject` and `ConstructedObject` each bind their declared object
/// variable. All other variants reference existing bindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub(crate) enum QueryPredicate {
    EventKind {
        event: VarId,
        expected: EventSpec,
    },
    EventIdentity {
        event: VarId,
        expected: IdentitySpec,
    },
    Argument {
        call: VarId,
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
    ReturnedObject {
        bind: VarId,
        identity: IdentitySpec,
    },
    ConstructedObject {
        bind: VarId,
        identity: IdentitySpec,
    },
    MemberSubject {
        event: VarId,
        object: VarId,
    },
}

/// A single event selection with identity and argument constraints.
///
/// This is a leaf predicate — it selects an event occurrence and optionally
/// constrains its identity and arguments. Evidence metadata is not stored
/// here; it lives in the owning [`EmissionDecl`].
///
/// Construct instances through the typed combinator methods such as
/// [`EventQuery::call_global`], [`EventQuery::member_call_rooted`], etc.
/// Then call [`into_query`](Self::into_query) to produce a [`QueryDecl`]
/// with inferred evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventQuery {
    /// Variable bound by this event selection.
    pub(crate) var: VarId,
    /// Kind of event (call, construct, member call, etc.).
    pub(crate) event: EventSpec,
    /// Identity specification (global, rooted, module, etc.).
    pub(crate) identity: IdentitySpec,
    /// Argument value constraints (empty for non-call events).
    pub(crate) constraints: Vec<ArgumentConstraint>,
}

impl EventQuery {
    pub fn var(&self) -> VarId {
        self.var
    }

    pub fn event(&self) -> &EventSpec {
        &self.event
    }

    pub fn identity(&self) -> &IdentitySpec {
        &self.identity
    }

    pub fn constraints(&self) -> &[ArgumentConstraint] {
        &self.constraints
    }
}

fn is_chain_malformed(chain: &str) -> bool {
    chain.trim().is_empty()
        || chain.contains("..")
        || chain.starts_with('.')
        || chain.ends_with('.')
}

fn validate_argument_constraints(
    constraints: &[ArgumentConstraint],
) -> Result<(), QueryBuildError> {
    let groups: BTreeSet<usize> = constraints.iter().map(ArgumentConstraint::index).collect();
    if groups.len() > limits::MAX_ARGUMENT_GROUPS {
        return Err(QueryBuildError::ExcessiveArgumentGroups(groups.len()));
    }
    for index in groups {
        let count = constraints
            .iter()
            .filter(|constraint| constraint.index() == index)
            .count();
        if count > limits::MAX_PREDICATES_PER_ARGUMENT {
            return Err(QueryBuildError::ExcessivePredicates { index, count });
        }
    }
    Ok(())
}

fn evidence_kind_for_event(event: &EventSpec) -> MatchKind {
    match event {
        EventSpec::Call => MatchKind::Call,
        EventSpec::Construct => MatchKind::Constructor,
        EventSpec::MemberCall { .. } => MatchKind::MemberCall,
        EventSpec::MemberRead { .. } => MatchKind::MemberRead,
        EventSpec::ClassReference => MatchKind::Class,
        EventSpec::Import => MatchKind::Import,
        EventSpec::StringReference => MatchKind::StringContains,
    }
}

#[allow(clippy::cast_possible_truncation)]
impl EventQuery {
    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global { name },
            constraints: Vec::new(),
        })
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Heuristic { name },
            constraints: Vec::new(),
        })
    }

    /// Module-export call.
    pub fn call_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if export.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::ModuleExport { module, export },
            constraints: Vec::new(),
        })
    }

    /// Package module export call.
    pub fn call_package(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let export: SmolStr = export.into().into();
        let module = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::PackageModuleExport { module, export },
            constraints: Vec::new(),
        })
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: path.clone(),
            },
            identity: IdentitySpec::Rooted { path },
            constraints: Vec::new(),
        })
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberCall { member: path },
            identity: IdentitySpec::Heuristic {
                name: chain_str.into(),
            },
            constraints: Vec::new(),
        })
    }

    /// Module-namespace member call.
    pub fn member_call_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let member_str: String = member.into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        let path = SymbolPath::from(member_str.as_str());
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberCall { member: path },
            identity: IdentitySpec::ModuleNamespace { module },
            constraints: Vec::new(),
        })
    }

    /// Package module namespace member call.
    pub fn member_call_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let member_str: String = member.into();
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        let path = SymbolPath::from(member_str.as_str());
        let module = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberCall { member: path },
            identity: IdentitySpec::PackageModuleNamespace { module },
            constraints: Vec::new(),
        })
    }

    /// Rooted member read.
    pub fn member_read_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberRead {
                member: path.clone(),
            },
            identity: IdentitySpec::Rooted { path },
            constraints: Vec::new(),
        })
    }

    /// Module-namespace member read.
    pub fn member_read_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let member_str: String = member.into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        let path = SymbolPath::from(member_str.as_str());
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberRead { member: path },
            identity: IdentitySpec::ModuleNamespace { module },
            constraints: Vec::new(),
        })
    }

    /// Package module namespace member read.
    pub fn member_read_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let member_str: String = member.into();
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        let path = SymbolPath::from(member_str.as_str());
        let module = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::MemberRead { member: path },
            identity: IdentitySpec::PackageModuleNamespace { module },
            constraints: Vec::new(),
        })
    }

    /// Import exact module specifier.
    pub fn import_exact(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let module_str: String = module.into();
        if module_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Import,
            identity: IdentitySpec::LiteralString {
                predicate: module_str,
            },
            constraints: Vec::new(),
        })
    }

    /// Import package pattern.
    pub fn import_package(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let pattern = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Import,
            identity: IdentitySpec::PackageSpecifier { pattern },
            constraints: Vec::new(),
        })
    }

    /// Static string reference.
    pub fn string_contains(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        let value_str: String = value.into();
        if value_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyStaticValue);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::StringReference,
            identity: IdentitySpec::LiteralString {
                predicate: value_str,
            },
            constraints: Vec::new(),
        })
    }

    /// Heuristic class reference.
    pub fn class_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::ClassReference,
            identity: IdentitySpec::Heuristic { name },
            constraints: Vec::new(),
        })
    }

    /// Module-export class reference.
    pub fn class_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if export.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::ClassReference,
            identity: IdentitySpec::ModuleExport { module, export },
            constraints: Vec::new(),
        })
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Construct,
            identity: IdentitySpec::Global { name },
            constraints: Vec::new(),
        })
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Construct,
            identity: IdentitySpec::Heuristic { name },
            constraints: Vec::new(),
        })
    }

    /// Module-export constructor.
    pub fn constructor_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if export.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self {
            var: VarId::new(0),
            event: EventSpec::Construct,
            identity: IdentitySpec::ModuleExport { module, export },
            constraints: Vec::new(),
        })
    }

    /// Add an argument predicate.
    pub fn with_arg(
        mut self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints
            .push(ArgumentConstraint::new(arg_idx, matcher));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Add a static-string argument constraint.
    pub fn with_arg_static_string(mut self, index: usize) -> Result<Self, QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints.push(ArgumentConstraint::new(
            arg_idx,
            ValueMatcher::static_string(),
        ));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Add a static-string constraint with allowed values.
    pub fn with_arg_static_strings<I, S>(
        mut self,
        index: usize,
        values: I,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints.push(ArgumentConstraint::new(
            arg_idx,
            ValueMatcher::static_string().equals_any(values)?,
        ));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Add a static-string contains constraint.
    pub fn with_arg_static_string_contains<I, S>(
        mut self,
        index: usize,
        values: I,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints.push(ArgumentConstraint::new(
            arg_idx,
            ValueMatcher::static_string().contains_any(values)?,
        ));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Add an object property value constraint.
    pub fn with_arg_object_property_value(
        mut self,
        index: usize,
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Result<Self, QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints.push(ArgumentConstraint::new(
            arg_idx,
            ArgumentMatcher::object_property_value(property, value),
        ));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Add an object keys constraint.
    pub fn with_arg_object_keys<I, S>(
        mut self,
        index: usize,
        keys: I,
    ) -> Result<Self, QueryBuildError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        let arg_idx = ArgumentIndex::new_unchecked(index as u8);
        self.constraints.push(ArgumentConstraint::new(
            arg_idx,
            ArgumentMatcher::object_keys(keys)?,
        ));
        validate_argument_constraints(&self.constraints)?;
        Ok(self)
    }

    /// Convert this event query into a [`QueryDecl`] with inferred evidence
    /// kind and symbol derived from the event and identity.
    pub fn into_query(self) -> QueryDecl {
        let var = self.var;
        let kind = evidence_kind_for_event(&self.event);
        let symbol = self.identity.display_name();
        QueryDecl {
            expression: QueryExpr::event(self),
            emission: EmissionDecl {
                primary_var: var,
                kind,
                symbol,
            },
        }
    }
}

/// Object lifecycle: source events, condition, and completion.
///
/// Represents a bounded state machine tracking an object from its sources
/// (production), through configuration (requirements), to completion (sink or
/// self-configuration). The compiler validates stages and compiles to the
/// existing local/cross-call flow engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleQuery {
    /// Evidence symbol.
    pub(crate) symbol: String,
    /// Events that produce the tracked object.
    pub(crate) sources: Vec<EventQuery>,
    /// Optional configuration condition (requirements).
    pub(crate) condition: Option<LifecycleCondition>,
    /// Optional completion mode (sink or configuration).
    pub(crate) completion: Option<LifecycleCompletion>,
}

impl LifecycleQuery {
    /// Create a lifecycle query with the given components.
    #[doc(hidden)]
    pub(crate) fn new(
        symbol: impl Into<String>,
        sources: Vec<EventQuery>,
        condition: Option<LifecycleCondition>,
        completion: Option<LifecycleCompletion>,
    ) -> Result<Self, QueryBuildError> {
        if sources.is_empty() {
            return Err(QueryBuildError::MissingLifecycleSources);
        }
        if sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                sources.len(),
            ));
        }
        let completion = completion.ok_or(QueryBuildError::MissingLifecycleCompletion)?;
        if let Some(condition) = &condition {
            let count = match condition.kind() {
                crate::api::rule::query::lifecycle::LifecycleConditionKind::AnyOf(events)
                | crate::api::rule::query::lifecycle::LifecycleConditionKind::AllOf(events) => {
                    if events.is_empty() {
                        return Err(QueryBuildError::EmptyLifecycleCondition);
                    }
                    events.len()
                }
            };
            if count > limits::MAX_LIFECYCLE_EVENTS {
                return Err(QueryBuildError::CollectionTooLarge(
                    "lifecycle condition events",
                    count,
                ));
            }
        }
        match completion.kind() {
            crate::api::rule::query::lifecycle::LifecycleCompletionKind::Configuration => {
                if condition.is_none() {
                    return Err(QueryBuildError::MissingLifecycleCondition);
                }
            }
            crate::api::rule::query::lifecycle::LifecycleCompletionKind::AnySink(sinks)
            | crate::api::rule::query::lifecycle::LifecycleCompletionKind::AllSinks(sinks) => {
                if sinks.is_empty() {
                    return Err(QueryBuildError::EmptyLifecycleSinks);
                }
                if sinks.len() > limits::MAX_LIFECYCLE_SINKS {
                    return Err(QueryBuildError::CollectionTooLarge(
                        "lifecycle completion sinks",
                        sinks.len(),
                    ));
                }
            }
        }
        Ok(Self {
            symbol: symbol.into(),
            sources,
            condition,
            completion: Some(completion),
        })
    }

    /// Return the evidence symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn sources(&self) -> &[EventQuery] {
        &self.sources
    }

    pub fn condition(&self) -> Option<&LifecycleCondition> {
        self.condition.as_ref()
    }

    pub fn completion(&self) -> Option<&LifecycleCompletion> {
        self.completion.as_ref()
    }
}

/// Evidence emission for a query result.
///
/// Every logical query must have exactly one emission declaration. It selects
/// the primary event variable and specifies the evidence kind and stable
/// symbol. The compiler validates that the primary variable exists on every
/// successful logical branch and has a source location.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmissionDecl {
    /// Variable holding the primary event for evidence.
    pub(crate) primary_var: VarId,
    /// Evidence kind (call, member call, flow, etc.).
    pub(crate) kind: MatchKind,
    /// Stable evidence symbol (e.g. "fetch", "document.createElement").
    pub(crate) symbol: String,
}

impl EmissionDecl {
    pub fn primary_var(&self) -> VarId {
        self.primary_var
    }

    pub fn kind(&self) -> MatchKind {
        self.kind
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }
}

/// A single constraint on a previously selected event, used with
/// [`QueryDecl::all`] to compose same-event conjunctions.
///
/// Each `EventRequirement` is a typed predicate that references an
/// already-bound event variable. The compiler groups requirements by
/// argument index and canonicalizes them during normalization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventRequirement {
    pub(crate) kind: EventRequirementKind,
}

/// Interior kind for [`EventRequirement`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum EventRequirementKind {
    Argument {
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
}

impl EventRequirement {
    /// Create an argument constraint on the event.
    ///
    /// The `index` is the 0-based argument position. Returns
    /// [`QueryBuildError::InvalidArgumentIndex`] when the index exceeds
    /// [`limits::MAX_ARGUMENT_INDEX`].
    pub fn argument(
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        #[allow(clippy::cast_possible_truncation)]
        let idx = index as u8;
        Ok(Self {
            kind: EventRequirementKind::Argument {
                index: ArgumentIndex::new_unchecked(idx),
                matcher: matcher.into(),
            },
        })
    }
}

/// A full logical query: an expression with its emission declaration.
///
/// One `QueryDecl` corresponds conceptually to one authored matcher:
///
/// ```text
/// select ... where ... emit ...
/// ```
///
/// The expression selects and constrains events; the emission declaration
/// specifies how the result produces evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryDecl {
    /// The query expression (event, any, all, lifecycle).
    pub(crate) expression: QueryExpr,
    /// How to emit evidence from the result.
    pub(crate) emission: EmissionDecl,
}

impl QueryDecl {
    pub fn expression(&self) -> &QueryExpr {
        &self.expression
    }

    pub fn emission(&self) -> &EmissionDecl {
        &self.emission
    }

    /// Member call on an instance created by a module export.
    pub fn member_call_instance(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module_str: SmolStr = module.into().into();
        let export_str: SmolStr = export.into().into();
        let member_str: String = member.into();
        if module_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if export_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        let event_var = VarId::new(0);
        let object_var = VarId::new(1);
        let member_path = SymbolPath::from(member_str.as_str());
        let symbol = format!("{module_str}.{export_str}");
        let identity = IdentitySpec::ModuleExport {
            module: module_str,
            export: export_str,
        };
        let branches = vec![
            QueryExpr::select_event(event_var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: event_var,
                expected: EventSpec::MemberCall {
                    member: member_path,
                },
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: event_var,
                expected: identity.clone(),
            }),
            QueryExpr::require(QueryPredicate::ConstructedObject {
                bind: object_var,
                identity,
            }),
            QueryExpr::require(QueryPredicate::MemberSubject {
                event: event_var,
                object: object_var,
            }),
        ];
        Ok(Self {
            expression: QueryExpr::all(AllExpr { branches }),
            emission: EmissionDecl {
                primary_var: event_var,
                kind: MatchKind::MemberCall,
                symbol,
            },
        })
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let member_str: String = member.into();
        if is_chain_malformed(&source_str) || is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(source_str));
        }
        let event_var = VarId::new(0);
        let object_var = VarId::new(1);
        let source_path = SymbolPath::from(source_str.as_str());
        let member_path = SymbolPath::from(member_str.as_str());
        let identity = IdentitySpec::Rooted { path: source_path };
        let branches = vec![
            QueryExpr::select_event(event_var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: event_var,
                expected: EventSpec::MemberCall {
                    member: member_path,
                },
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: event_var,
                expected: identity.clone(),
            }),
            QueryExpr::require(QueryPredicate::ReturnedObject {
                bind: object_var,
                identity,
            }),
            QueryExpr::require(QueryPredicate::MemberSubject {
                event: event_var,
                object: object_var,
            }),
        ];
        Ok(Self {
            expression: QueryExpr::all(AllExpr { branches }),
            emission: EmissionDecl {
                primary_var: event_var,
                kind: MatchKind::MemberCall,
                symbol: source_str,
            },
        })
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source_str: String = source.into();
        let member_str: String = member.into();
        if is_chain_malformed(&source_str) || is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(source_str));
        }
        let event_var = VarId::new(0);
        let object_var = VarId::new(1);
        let source_path = SymbolPath::from(source_str.as_str());
        let member_path = SymbolPath::from(member_str.as_str());
        let identity = IdentitySpec::Rooted { path: source_path };
        let branches = vec![
            QueryExpr::select_event(event_var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: event_var,
                expected: EventSpec::MemberRead {
                    member: member_path,
                },
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: event_var,
                expected: identity.clone(),
            }),
            QueryExpr::require(QueryPredicate::ReturnedObject {
                bind: object_var,
                identity,
            }),
            QueryExpr::require(QueryPredicate::MemberSubject {
                event: event_var,
                object: object_var,
            }),
        ];
        Ok(Self {
            expression: QueryExpr::all(AllExpr { branches }),
            emission: EmissionDecl {
                primary_var: event_var,
                kind: MatchKind::MemberRead,
                symbol: source_str,
            },
        })
    }

    // ── Evidence override ─────────────────────────────────────────

    /// Override the evidence kind and symbol.
    #[cfg(test)]
    pub(crate) fn with_evidence(mut self, kind: MatchKind, symbol: impl Into<String>) -> Self {
        self.emission.kind = kind;
        self.emission.symbol = symbol.into();
        self
    }

    /// Construct an `Any` expression from an iterable of fallible query
    /// declarations.
    ///
    /// Each branch is a [`QueryDecl`] or
    /// `Result<QueryDecl, QueryBuildError>`. Returns
    /// [`QueryBuildError::EmptyAlternatives`] if the iterator yields no
    /// branches. Branch scopes are independent: the same variable name may
    /// be bound in different branches with compatible types.
    ///
    /// # Example
    ///
    /// ```ignore
    /// QueryDecl::any([
    ///     EventQuery::call_global("fetch").map(EventQuery::into_query),
    ///     EventQuery::call_global("navigate").map(EventQuery::into_query),
    /// ])?;
    /// ```
    pub fn any(
        branches: impl IntoIterator<Item = Result<Self, QueryBuildError>>,
    ) -> Result<Self, QueryBuildError> {
        let mut exprs = Vec::new();
        let mut first_emission: Option<EmissionDecl> = None;
        for branch in branches {
            let decl = branch?;
            if let Some(first) = &first_emission {
                let primary_present = decl.expression.vars().contains(&first.primary_var);
                if !primary_present
                    || decl.emission.primary_var != first.primary_var
                    || decl.emission.kind != first.kind
                {
                    return Err(QueryBuildError::EvidenceProjection);
                }
            } else {
                first_emission = Some(decl.emission.clone());
            }
            exprs.push(decl.expression);
        }
        if exprs.is_empty() {
            return Err(QueryBuildError::EmptyAlternatives);
        }
        let first = first_emission.unwrap_or_else(Self::default_emission);
        Ok(Self {
            expression: QueryExpr::any(AnyExpr::new(exprs)?),
            emission: first,
        })
    }

    /// Construct a same-event `All` expression from one event selection
    /// and zero or more [`EventRequirement`] constraints.
    ///
    /// The result is an `All` with the event selection and requirement atoms
    /// as branches. Uncorrelated multi-event conjunctions are rejected
    /// during validation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// QueryDecl::all(
    ///     EventQuery::call_global("fetch"),
    ///     [EventRequirement::argument(0, ValueMatcher::static_string())?],
    /// )?;
    /// ```
    pub fn all(
        event: Result<EventQuery, QueryBuildError>,
        requirements: impl IntoIterator<Item = Result<EventRequirement, QueryBuildError>>,
    ) -> Result<Self, QueryBuildError> {
        let eq = event?;
        let var = eq.var;
        let kind = evidence_kind_for_event(&eq.event);
        let symbol = eq.identity.display_name();

        // Build All branches: SelectEvent + EventKind + EventIdentity +
        // argument constraints as Require atoms.
        let event_spec = eq.event;
        let identity_spec = eq.identity;
        let mut branches: Vec<QueryExpr> = vec![
            QueryExpr::select_event(var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: var,
                expected: event_spec,
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: var,
                expected: identity_spec,
            }),
        ];

        for req_result in requirements {
            let req = req_result?;
            match req.kind {
                EventRequirementKind::Argument { index, matcher } => {
                    branches.push(QueryExpr::require(QueryPredicate::Argument {
                        call: var,
                        index,
                        matcher,
                    }));
                }
            }
        }

        let expression = QueryExpr::all(AllExpr::new(branches)?);
        Ok(Self {
            expression,
            emission: EmissionDecl {
                primary_var: var,
                kind,
                symbol,
            },
        })
    }

    /// Default emission for placeholder use.
    fn default_emission() -> EmissionDecl {
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: String::new(),
        }
    }

    /// Wrap a [`LifecycleQuery`] into a [`QueryDecl`] with inferred evidence.
    /// Accepts a `Result` from a builder for direct use in
    /// [`RuleBuilder::query`].
    pub fn lifecycle(
        lc_result: Result<LifecycleQuery, QueryBuildError>,
    ) -> Result<Self, QueryBuildError> {
        lc_result.map(|lc| {
            let symbol = lc.symbol.clone();
            debug_assert!(!symbol.trim().is_empty());
            Self {
                expression: QueryExpr::lifecycle(lc),
                emission: EmissionDecl {
                    primary_var: VarId::new(0),
                    kind: MatchKind::CallArgument,
                    symbol,
                },
            }
        })
    }
}

/// Sealed trait allowing [`RuleBuilder::query`] to accept a [`QueryDecl`],
/// [`EventQuery`], or `Result` of either without requiring the caller to
/// unwrap.
pub trait IntoQueryDecl: private::Sealed {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError>;
}

impl IntoQueryDecl for QueryDecl {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        Ok(self)
    }
}

impl IntoQueryDecl for Result<QueryDecl, QueryBuildError> {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        self
    }
}

impl IntoQueryDecl for EventQuery {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        Ok(self.into_query())
    }
}

impl IntoQueryDecl for Result<EventQuery, QueryBuildError> {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        self.map(EventQuery::into_query)
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::QueryDecl {}
    impl Sealed for Result<super::QueryDecl, super::QueryBuildError> {}
    impl Sealed for super::EventQuery {}
    impl Sealed for Result<super::EventQuery, super::QueryBuildError> {}
}

impl fmt::Display for QueryDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query emit={} {}", self.emission.symbol, self.expression)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{classification::MatchKind, rule::ValueMatcher};

    // ── VarId tests ────────────────────────────────────────────────

    #[test]
    fn var_id_new_round_trips() {
        let id = VarId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn var_id_ordering_is_stable() {
        let a = VarId::new(1);
        let b = VarId::new(2);
        assert!(a < b);
    }

    // ── AnyExpr / AllExpr empty rejection ──────────────────────────

    #[test]
    fn any_expr_rejects_empty_branches() {
        assert_eq!(
            AnyExpr::new(vec![]),
            Err(QueryBuildError::EmptyAlternatives)
        );
    }

    #[test]
    fn all_expr_rejects_empty_branches() {
        assert_eq!(AllExpr::new(vec![]), Err(QueryBuildError::EmptyConjunction));
    }

    #[test]
    fn any_expr_accepts_non_empty_branches() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let any = AnyExpr::new(vec![event.clone(), event]).unwrap();
        assert_eq!(any.branches.len(), 2);
    }

    #[test]
    fn all_expr_accepts_non_empty_branches() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let all = AllExpr::new(vec![event]).unwrap();
        assert_eq!(all.branches.len(), 1);
    }

    #[test]
    fn expression_depth_is_bounded_before_compilation() {
        let mut nested = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        for _ in 1..limits::MAX_EXPR_DEPTH {
            nested = QueryExpr::any(AnyExpr::new(vec![nested]).unwrap());
        }

        assert!(matches!(
            AnyExpr::new(vec![nested]),
            Err(QueryBuildError::ExpressionDepthExceeded(depth))
                if depth == limits::MAX_EXPR_DEPTH + 1
        ));
    }

    #[test]
    fn expression_child_limit_plus_one_is_rejected_at_authoring() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let branches = vec![event; limits::MAX_EXPR_CHILDREN + 1];
        assert!(matches!(
            AnyExpr::new(branches),
            Err(QueryBuildError::CollectionTooLarge(
                "Any expression branches",
                257
            ))
        ));
    }

    // ── Construction: every EventQuery constructor → valid QueryDecl ──

    #[allow(clippy::needless_pass_by_value)]
    fn assert_event_query(decl: QueryDecl, expected_symbol: &str) {
        assert_eq!(decl.emission.primary_var, VarId::new(0));
        assert_eq!(decl.emission.symbol, expected_symbol);
        assert!(matches!(&decl.expression.kind, QueryExprKind::Event(_)));
    }

    #[allow(clippy::needless_pass_by_value)]
    fn assert_any_all_query(decl: QueryDecl, expected_symbol: &str) {
        assert_eq!(decl.emission.primary_var, VarId::new(0));
        assert_eq!(decl.emission.symbol, expected_symbol);
    }

    #[test]
    fn lowers_call_global_to_query_decl() {
        assert_event_query(
            EventQuery::call_global("fetch").unwrap().into_query(),
            "fetch",
        );
    }

    #[test]
    fn lowers_call_heuristic_to_query_decl() {
        assert_event_query(
            EventQuery::call_heuristic("fetch").unwrap().into_query(),
            "fetch",
        );
    }

    #[test]
    fn lowers_call_module_to_query_decl() {
        assert_event_query(
            EventQuery::call_module("fs", "readFile")
                .unwrap()
                .into_query(),
            "fs.readFile",
        );
    }

    #[test]
    fn lowers_call_package_to_query_decl() {
        assert_event_query(
            EventQuery::call_package("@scope/pkg", "method")
                .unwrap()
                .into_query(),
            "@scope/pkg.method",
        );
    }

    #[test]
    fn lowers_member_call_rooted_to_query_decl() {
        assert_event_query(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .into_query(),
            "document.createElement",
        );
    }

    #[test]
    fn lowers_member_call_heuristic_to_query_decl() {
        assert_event_query(
            EventQuery::member_call_heuristic("foo.bar")
                .unwrap()
                .into_query(),
            "foo.bar",
        );
    }

    #[test]
    fn lowers_member_call_module_to_query_decl() {
        assert_event_query(
            EventQuery::member_call_module("module", "method")
                .unwrap()
                .into_query(),
            "module",
        );
    }

    #[test]
    fn lowers_member_call_instance_to_query_decl() {
        assert_any_all_query(
            QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
            "pkg.Client",
        );
    }

    #[test]
    fn lowers_member_call_package_to_query_decl() {
        assert_event_query(
            EventQuery::member_call_package("@scope/pkg", "method")
                .unwrap()
                .into_query(),
            "@scope/pkg",
        );
    }

    #[test]
    fn lowers_member_call_returned_to_query_decl() {
        assert_any_all_query(
            QueryDecl::member_call_returned("create", "send").unwrap(),
            "create",
        );
    }

    #[test]
    fn lowers_member_read_rooted_to_query_decl() {
        assert_event_query(
            EventQuery::member_read_rooted("window.location")
                .unwrap()
                .into_query(),
            "window.location",
        );
    }

    #[test]
    fn lowers_member_read_module_to_query_decl() {
        assert_event_query(
            EventQuery::member_read_module("module", "property")
                .unwrap()
                .into_query(),
            "module",
        );
    }

    #[test]
    fn lowers_member_read_returned_to_query_decl() {
        assert_any_all_query(
            QueryDecl::member_read_returned("create", "token").unwrap(),
            "create",
        );
    }

    #[test]
    fn lowers_member_read_package_to_query_decl() {
        assert_event_query(
            EventQuery::member_read_package("@scope/pkg", "property")
                .unwrap()
                .into_query(),
            "@scope/pkg",
        );
    }

    #[test]
    fn lowers_import_exact_to_query_decl() {
        assert_event_query(
            EventQuery::import_exact("node:fs").unwrap().into_query(),
            "node:fs",
        );
    }

    #[test]
    fn lowers_import_package_to_query_decl() {
        assert_event_query(
            EventQuery::import_package("@scope/pkg")
                .unwrap()
                .into_query(),
            "@scope/pkg",
        );
    }

    #[test]
    fn lowers_string_contains_to_query_decl() {
        assert_event_query(
            EventQuery::string_contains("https://")
                .unwrap()
                .into_query(),
            "https://",
        );
    }

    #[test]
    fn lowers_class_heuristic_to_query_decl() {
        assert_event_query(
            EventQuery::class_heuristic("Worker").unwrap().into_query(),
            "Worker",
        );
    }

    #[test]
    fn lowers_class_module_to_query_decl() {
        assert_event_query(
            EventQuery::class_module("module", "Klass")
                .unwrap()
                .into_query(),
            "module.Klass",
        );
    }

    #[test]
    fn lowers_constructor_global_to_query_decl() {
        assert_event_query(
            EventQuery::constructor_global("URL").unwrap().into_query(),
            "URL",
        );
    }

    #[test]
    fn lowers_constructor_heuristic_to_query_decl() {
        assert_event_query(
            EventQuery::constructor_heuristic("Foo")
                .unwrap()
                .into_query(),
            "Foo",
        );
    }

    #[test]
    fn lowers_constructor_module_to_query_decl() {
        assert_event_query(
            EventQuery::constructor_module("pkg", "Klass")
                .unwrap()
                .into_query(),
            "pkg.Klass",
        );
    }

    #[test]
    fn lowers_arg_constraints_to_query_decl() {
        let q = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .with_arg_static_string(1)
            .unwrap()
            .with_arg_static_strings(2, ["a", "b"])
            .unwrap()
            .with_arg_static_string_contains(3, ["token"])
            .unwrap()
            .into_query();
        match &q.expression.kind {
            QueryExprKind::Event(eq) => {
                assert_eq!(eq.constraints.len(), 4);
            }
            _ => panic!("expected Event expression"),
        }
    }

    #[test]
    fn lowers_evidence_override_to_query_decl() {
        let q = EventQuery::call_global("fetch")
            .unwrap()
            .into_query()
            .with_evidence(MatchKind::CallArgument, "custom.fetch");
        assert_eq!(q.emission.kind, MatchKind::CallArgument);
        assert_eq!(q.emission.symbol, "custom.fetch");
    }

    // ── Equivalent forms produce equivalent declarations ──────────

    #[test]
    fn semantically_equivalent_decls_lower_equally() {
        let q_a = EventQuery::call_global("fetch").unwrap().into_query();
        let q_b = EventQuery::call_global("fetch").unwrap().into_query();
        assert_eq!(q_a, q_b);
    }

    // ── Diagnostic names ──────────────────────────────────────────

    #[test]
    fn query_expr_diagnostic_names_are_stable() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        assert_eq!(event.diagnostic_name(), "event");

        let any = QueryExpr::any(AnyExpr::new(vec![event.clone()]).unwrap());
        assert_eq!(any.diagnostic_name(), "any");

        let all = QueryExpr::all(AllExpr::new(vec![event]).unwrap());
        assert_eq!(all.diagnostic_name(), "all");
    }

    #[test]
    fn event_spec_diagnostic_names_are_stable() {
        assert_eq!(EventSpec::Call.diagnostic_name(), "call");
        assert_eq!(EventSpec::Construct.diagnostic_name(), "construct");
        assert_eq!(
            EventSpec::MemberCall {
                member: SymbolPath::from("foo")
            }
            .diagnostic_name(),
            "member_call"
        );
        assert_eq!(
            EventSpec::MemberRead {
                member: SymbolPath::from("foo")
            }
            .diagnostic_name(),
            "member_read"
        );
        assert_eq!(EventSpec::ClassReference.diagnostic_name(), "class");
        assert_eq!(EventSpec::Import.diagnostic_name(), "import");
        assert_eq!(EventSpec::StringReference.diagnostic_name(), "string");
    }

    #[test]
    fn identity_spec_diagnostic_names_are_stable() {
        assert_eq!(
            IdentitySpec::Global {
                name: SmolStr::new("f")
            }
            .diagnostic_name(),
            "global"
        );
        assert_eq!(
            IdentitySpec::Heuristic {
                name: SmolStr::new("f")
            }
            .diagnostic_name(),
            "heuristic"
        );
        assert_eq!(
            IdentitySpec::ModuleExport {
                module: SmolStr::new("m"),
                export: SmolStr::new("e")
            }
            .diagnostic_name(),
            "module_export"
        );
        assert_eq!(
            IdentitySpec::Rooted {
                path: SymbolPath::from("a.b")
            }
            .diagnostic_name(),
            "rooted"
        );
        assert_eq!(
            IdentitySpec::LiteralString {
                predicate: "s".into()
            }
            .diagnostic_name(),
            "literal"
        );
    }

    // ── Display and plan summary ──────────────────────────────────

    #[test]
    fn query_expr_display_shapes_are_compact() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let text = format!("{event}");
        assert!(text.contains("select"));
        assert!(text.contains("$0"));
        assert!(text.contains("call"));
        assert!(text.contains("global"));
    }

    #[test]
    fn any_display_shows_branches() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let any = QueryExpr::any(AnyExpr::new(vec![event]).unwrap());
        let text = format!("{any}");
        assert!(text.starts_with("any ["));
        assert!(text.ends_with(']'));
    }

    #[test]
    fn query_decl_display_includes_symbol() {
        let q = EventQuery::call_global("fetch").unwrap().into_query();
        let text = format!("{q}");
        assert!(text.contains("fetch"));
    }

    #[test]
    fn queries_lower_correctly() {
        let queries = [
            EventQuery::call_global("fetch").unwrap().into_query(),
            EventQuery::member_read_rooted("window.location")
                .unwrap()
                .into_query(),
        ];
        assert_eq!(queries.len(), 2);
    }

    // ── VarId collection ──────────────────────────────────────────

    #[test]
    fn event_query_vars_contains_one() {
        let event = QueryExpr::event(EventQuery {
            var: VarId::new(5),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            constraints: vec![],
        });
        assert_eq!(event.vars(), vec![VarId::new(5)]);
    }

    #[test]
    fn any_query_vars_collects_all_branch_vars() {
        let a = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            constraints: vec![],
        });
        let b = QueryExpr::event(EventQuery {
            var: VarId::new(1),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("g"),
            },
            constraints: vec![],
        });
        let any = QueryExpr::any(AnyExpr::new(vec![a, b]).unwrap());
        let vars = any.vars();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&VarId::new(0)));
        assert!(vars.contains(&VarId::new(1)));
    }
}
