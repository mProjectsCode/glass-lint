//! Declaration-owned query semantics and typed logical query algebra.
//!
//! Types in this module are provider-neutral and validated at construction.
//! They represent authored intent without exposing compiler IR. The compiler
//! layer lowers these into physical execution plans.
//!
//! The primary authoring API consists of constructor methods on [`EventQuery`]
//! and convenience constructors on [`QueryDecl`]. Rule authors compose queries
//! and pass them to [`crate::api::rule::RuleBuilder::query`].
//!
//! [`EventQuery::call_global`], [`QueryDecl::call_global`], and the other
//! identity/event combinators replace the former [`QueryDecl`] builder.
use std::fmt;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, FlowCompletion, FlowCondition,
        ModuleSpecifierPattern, ObjectFlowMatcher, ValueMatcher,
    },
};

pub(crate) mod limits;

/// Declaration-owned identity specification for an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IdentitySpec {
    Global {
        name: SmolStr,
    },
    Heuristic {
        name: SmolStr,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    PackageModuleExport {
        module: ModuleSpecifierPattern,
        export: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    PackageModuleNamespace {
        module: ModuleSpecifierPattern,
    },
    Rooted {
        path: SymbolPath,
    },
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
}

impl IdentitySpec {
    /// Return a display-oriented string for the identity name.
    pub fn display_name(&self) -> String {
        match self {
            Self::Global { name } | Self::Heuristic { name } => name.to_string(),
            Self::ModuleExport { module, export } => format!("{module}.{export}"),
            Self::PackageModuleExport { module, export } => format!("{module}.{export}"),
            Self::ModuleNamespace { module } => module.to_string(),
            Self::PackageModuleNamespace { module } => module.to_string(),
            Self::Rooted { path } => path.to_string(),
            Self::LiteralString { predicate } => predicate.clone(),
            Self::PackageSpecifier { pattern } => pattern.to_string(),
        }
    }

    /// Stable diagnostic name for this identity kind.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Global { .. } => "global",
            Self::Heuristic { .. } => "heuristic",
            Self::ModuleExport { .. } => "module_export",
            Self::PackageModuleExport { .. } => "package_module_export",
            Self::ModuleNamespace { .. } => "module_namespace",
            Self::PackageModuleNamespace { .. } => "package_module_namespace",
            Self::Rooted { .. } => "rooted",
            Self::LiteralString { .. } => "literal",
            Self::PackageSpecifier { .. } => "package_specifier",
        }
    }
}

/// Declaration-owned event kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum EventSpec {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

impl EventSpec {
    /// Stable diagnostic name for this event kind.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Construct => "construct",
            Self::MemberCall { .. } => "member_call",
            Self::MemberRead { .. } => "member_read",
            Self::ClassReference => "class",
            Self::Import => "import",
            Self::StringReference => "string",
        }
    }
}

/// Declaration-owned subject relationship.
///
/// The identity for the producer (returned-from) or constructor (instance-of)
/// lives in [`EventQuery::identity`] — it is not duplicated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum SubjectSpec {
    /// The event is directly on the identity.
    Direct,
    /// The event is on an object returned from a producer.
    ReturnedFrom,
    /// The event is on an instance created by a constructor.
    InstanceOf,
}

impl SubjectSpec {
    /// Stable diagnostic name for this subject relationship.
    pub fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ReturnedFrom => "returned_from",
            Self::InstanceOf => "instance_of",
        }
    }
}

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

/// A single event selection with identity, subject, and argument constraints.
///
/// This is a leaf predicate — it selects an event occurrence and optionally
/// constrains its identity, subject relationship, and arguments. Evidence
/// metadata is not stored here; it lives in the owning [`EmissionDecl`].
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
    /// Subject relationship (direct, returned-from, instance-of).
    pub(crate) subject: SubjectSpec,
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

    pub fn subject(&self) -> SubjectSpec {
        self.subject
    }

    pub fn constraints(&self) -> &[ArgumentConstraint] {
        &self.constraints
    }

    /// Create an EventQuery with a different variable ID.
    /// Useful when composing multi-variable query expressions.
    pub fn with_var(mut self, var: VarId) -> Result<Self, QueryBuildError> {
        self.var = var;
        Ok(self)
    }
}

fn is_chain_malformed(chain: &str) -> bool {
    chain.trim().is_empty()
        || chain.contains("..")
        || chain.starts_with('.')
        || chain.ends_with('.')
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
    fn new(var: VarId, event: EventSpec, identity: IdentitySpec, subject: SubjectSpec) -> Self {
        Self {
            var,
            event,
            identity,
            subject,
            constraints: Vec::new(),
        }
    }

    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Call,
            IdentitySpec::Global { name },
            SubjectSpec::Direct,
        ))
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Call,
            IdentitySpec::Heuristic { name },
            SubjectSpec::Direct,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Call,
            IdentitySpec::ModuleExport { module, export },
            SubjectSpec::Direct,
        ))
    }

    /// Package module export call.
    pub fn call_package(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let export: SmolStr = export.into().into();
        let module = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Call,
            IdentitySpec::PackageModuleExport { module, export },
            SubjectSpec::Direct,
        ))
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall {
                member: path.clone(),
            },
            IdentitySpec::Rooted { path },
            SubjectSpec::Direct,
        ))
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall { member: path },
            IdentitySpec::Heuristic {
                name: chain_str.into(),
            },
            SubjectSpec::Direct,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall { member: path },
            IdentitySpec::ModuleNamespace { module },
            SubjectSpec::Direct,
        ))
    }

    /// Member call on an instance created by a module export.
    pub fn member_call_instance(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        let member_str: String = member.into();
        if module.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        if export.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        if is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(member_str));
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall {
                member: SymbolPath::from(member_str.as_str()),
            },
            IdentitySpec::ModuleExport { module, export },
            SubjectSpec::InstanceOf,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall { member: path },
            IdentitySpec::PackageModuleNamespace { module },
            SubjectSpec::Direct,
        ))
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source = source.into();
        let member: SmolStr = member.into().into();
        let member_str = member.to_string();
        if is_chain_malformed(&source) || is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(source));
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberCall {
                member: SymbolPath::from(member_str.as_str()),
            },
            IdentitySpec::Rooted {
                path: SymbolPath::from(source.as_str()),
            },
            SubjectSpec::ReturnedFrom,
        ))
    }

    /// Rooted member read.
    pub fn member_read_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        let chain_str: String = chain.into();
        if is_chain_malformed(&chain_str) {
            return Err(QueryBuildError::MalformedChain(chain_str));
        }
        let path = SymbolPath::from(chain_str.as_str());
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberRead {
                member: path.clone(),
            },
            IdentitySpec::Rooted { path },
            SubjectSpec::Direct,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberRead { member: path },
            IdentitySpec::ModuleNamespace { module },
            SubjectSpec::Direct,
        ))
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        let source = source.into();
        let member_str: String = member.into();
        if is_chain_malformed(&source) || is_chain_malformed(&member_str) {
            return Err(QueryBuildError::MalformedChain(source));
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberRead {
                member: SymbolPath::from(member_str.as_str()),
            },
            IdentitySpec::Rooted {
                path: SymbolPath::from(source.as_str()),
            },
            SubjectSpec::ReturnedFrom,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::MemberRead { member: path },
            IdentitySpec::PackageModuleNamespace { module },
            SubjectSpec::Direct,
        ))
    }

    /// Import exact module specifier.
    pub fn import_exact(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let module_str: String = module.into();
        if module_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyModuleSpecifier);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Import,
            IdentitySpec::LiteralString {
                predicate: module_str,
            },
            SubjectSpec::Direct,
        ))
    }

    /// Import package pattern.
    pub fn import_package(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        let pattern = ModuleSpecifierPattern::package(module)
            .map_err(|_| QueryBuildError::InvalidScopePackage)?;
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Import,
            IdentitySpec::PackageSpecifier { pattern },
            SubjectSpec::Direct,
        ))
    }

    /// Static string reference.
    pub fn string_contains(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        let value_str: String = value.into();
        if value_str.trim().is_empty() {
            return Err(QueryBuildError::EmptyStaticValue);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::StringReference,
            IdentitySpec::LiteralString {
                predicate: value_str,
            },
            SubjectSpec::Direct,
        ))
    }

    /// Heuristic class reference.
    pub fn class_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::ClassReference,
            IdentitySpec::Heuristic { name },
            SubjectSpec::Direct,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::ClassReference,
            IdentitySpec::ModuleExport { module, export },
            SubjectSpec::Direct,
        ))
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Construct,
            IdentitySpec::Global { name },
            SubjectSpec::Direct,
        ))
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            return Err(QueryBuildError::EmptyIdentityName);
        }
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Construct,
            IdentitySpec::Heuristic { name },
            SubjectSpec::Direct,
        ))
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
        Ok(Self::new(
            VarId::new(0),
            EventSpec::Construct,
            IdentitySpec::ModuleExport { module, export },
            SubjectSpec::Direct,
        ))
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
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
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
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
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
            ValueMatcher::static_string().equals_any(values),
        ));
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
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
            ValueMatcher::static_string().contains_any(values),
        ));
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
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
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
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
            ArgumentMatcher::object_keys(keys),
        ));
        if self.constraints.len()
            > limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS
        {
            return Err(QueryBuildError::ExcessiveConstraints(
                self.constraints.len(),
            ));
        }
        Ok(self)
    }

    /// Convert this event query into a [`QueryDecl`] with inferred evidence
    /// kind and symbol derived from the event and identity.
    pub fn into_query(self) -> QueryDecl {
        let var = self.var;
        let kind = evidence_kind_for_event(&self.event);
        let symbol = self.identity.display_name();
        QueryDecl {
            expression: QueryExpr::Event(self),
            emission: EmissionDecl {
                primary_var: var,
                kind,
                symbol,
            },
        }
    }
}

/// A typed logical query expression.
///
/// The algebra supports four operators:
///
/// - `Event` — select a single event with identity, subject, and constraints;
/// - `Any` — union of independently complete alternatives;
/// - `All` — conjunction over predicates sharing explicitly compatible
///   variables; and
/// - `Lifecycle` — object lifecycle with source, condition, and completion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryExpr {
    /// A single event with identity, subject, and constraints.
    Event(EventQuery),
    /// Union of alternative expressions (must be non-empty).
    Any(AnyExpr),
    /// Conjunction of expressions sharing compatible variables (must be
    /// non-empty; correlation required for multi-event joins).
    All(AllExpr),
    /// Object lifecycle: source event, condition, and completion.
    Lifecycle(LifecycleQuery),
}

impl QueryExpr {
    /// Stable diagnostic name for this operator.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Event(_) => "event",
            Self::Any(_) => "any",
            Self::All(_) => "all",
            Self::Lifecycle(_) => "lifecycle",
        }
    }

    /// Collect all variable IDs bound by this expression.
    pub fn vars(&self) -> Vec<VarId> {
        let mut ids = Vec::new();
        self.collect_vars(&mut ids);
        ids
    }

    fn collect_vars(&self, ids: &mut Vec<VarId>) {
        match self {
            Self::Event(q) => ids.push(q.var),
            Self::Any(a) => {
                for b in &a.branches {
                    b.collect_vars(ids);
                }
            }
            Self::All(a) => {
                for b in &a.branches {
                    b.collect_vars(ids);
                }
            }
            Self::Lifecycle(l) => {
                for src in &l.sources {
                    ids.push(src.var);
                }
            }
        }
    }
}

impl fmt::Display for QueryExpr {
    /// Compact debug-oriented display showing the expression shape.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event(q) => {
                write!(
                    f,
                    "select {} {} {} {}",
                    q.var,
                    q.event.diagnostic_name(),
                    q.identity.diagnostic_name(),
                    q.subject.diagnostic_name()
                )
            }
            Self::Any(a) => {
                write!(f, "any [")?;
                for (i, b) in a.branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{b}")?;
                }
                write!(f, "]")
            }
            Self::All(a) => {
                write!(f, "all [")?;
                for (i, b) in a.branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{b}")?;
                }
                write!(f, "]")
            }
            Self::Lifecycle(l) => {
                write!(
                    f,
                    "lifecycle sources={} condition={} completion={}",
                    l.sources.len(),
                    l.condition.is_some(),
                    l.completion.is_some()
                )
            }
        }
    }
}

/// Union of alternative query expressions.
///
/// Semantics:
/// - Normalize nested `Any`.
/// - Reject empty `Any`.
/// - Deduplicate equivalent branches.
/// - Preserve deterministic branch order.
/// - Merge duplicate results through the existing certainty/evidence policy.
/// - Do not let an unknown branch erase an independent complete witness.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnyExpr {
    pub(crate) branches: Vec<QueryExpr>,
}

impl AnyExpr {
    /// Create an `Any` expression. Returns an error if branches is empty.
    pub fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        if branches.is_empty() {
            return Err(QueryBuildError::EmptyAlternatives);
        }
        Ok(Self { branches })
    }

    /// Borrow the branches of this `Any` expression.
    pub fn branches(&self) -> &[QueryExpr] {
        &self.branches
    }
}

/// Conjunction of query expressions sharing compatible variables.
///
/// Semantics:
/// - Normalize nested `All`.
/// - Reject empty `All`.
/// - Attach single-event filters to the selecting scan where possible.
/// - Require explicit shared variables for multi-event joins (checked during
///   validation).
/// - Preserve one path-correlation token across all contributing predicates.
/// - Never combine evidence from incompatible alternatives.
/// - Propagate incomplete state without fabricating a witness.
/// - Select one explicit primary event for result emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AllExpr {
    pub(crate) branches: Vec<QueryExpr>,
}

impl AllExpr {
    /// Create an `All` expression. Returns an error if branches is empty.
    pub fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        if branches.is_empty() {
            return Err(QueryBuildError::EmptyConjunction);
        }
        Ok(Self { branches })
    }

    /// Borrow the branches of this `All` expression.
    pub fn branches(&self) -> &[QueryExpr] {
        &self.branches
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
    /// Events that produce the tracked object.
    pub(crate) sources: Vec<EventQuery>,
    /// Optional configuration condition (requirements).
    pub(crate) condition: Option<FlowCondition>,
    /// Optional completion mode (sink or configuration).
    pub(crate) completion: Option<FlowCompletion>,
}

impl LifecycleQuery {
    /// Create a new lifecycle query with the given sources, condition, and
    /// completion.
    ///
    /// The sources must be non-empty and within
    /// [`limits::MAX_LIFECYCLE_SOURCES`].
    pub fn new(
        sources: Vec<EventQuery>,
        condition: Option<FlowCondition>,
        completion: Option<FlowCompletion>,
    ) -> Result<Self, QueryBuildError> {
        if sources.is_empty() {
            return Err(QueryBuildError::EmptyCollection("lifecycle sources"));
        }
        if sources.len() > limits::MAX_LIFECYCLE_SOURCES {
            return Err(QueryBuildError::CollectionTooLarge(
                "lifecycle sources",
                sources.len(),
            ));
        }
        Ok(Self {
            sources,
            condition,
            completion,
        })
    }

    pub fn sources(&self) -> &[EventQuery] {
        &self.sources
    }

    pub fn condition(&self) -> Option<&FlowCondition> {
        self.condition.as_ref()
    }

    pub fn completion(&self) -> Option<&FlowCompletion> {
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
    /// Create a query declaration from an [`EventQuery`], inferring evidence
    /// from the event kind and identity.
    pub fn from_event_query(event_query: EventQuery) -> Self {
        event_query.into_query()
    }

    pub fn expression(&self) -> &QueryExpr {
        &self.expression
    }

    pub fn emission(&self) -> &EmissionDecl {
        &self.emission
    }

    // ── Convenience constructors ──────────────────────────────────
    //
    // These are thin wrappers over the corresponding `EventQuery` constructor
    // followed by `into_query()`. They replace the former
    // `QueryDecl::builder().<method>().build().unwrap()` pattern.

    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::call_global(name)?.into_query())
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::call_heuristic(name)?.into_query())
    }

    /// Module-export call.
    pub fn call_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::call_module(module, export)?.into_query())
    }

    /// Package module export call.
    pub fn call_package(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::call_package(module, export)?.into_query())
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_rooted(chain)?.into_query())
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_heuristic(chain)?.into_query())
    }

    /// Module-namespace member call.
    pub fn member_call_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_module(module, member)?.into_query())
    }

    /// Member call on an instance created by a module export.
    pub fn member_call_instance(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_instance(module, export, member)?.into_query())
    }

    /// Package module namespace member call.
    pub fn member_call_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_package(module, member)?.into_query())
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_call_returned(source, member)?.into_query())
    }

    /// Rooted member read.
    pub fn member_read_rooted(chain: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_read_rooted(chain)?.into_query())
    }

    /// Module-namespace member read.
    pub fn member_read_module(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_read_module(module, member)?.into_query())
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_read_returned(source, member)?.into_query())
    }

    /// Package module namespace member read.
    pub fn member_read_package(
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::member_read_package(module, member)?.into_query())
    }

    /// Import exact module specifier.
    pub fn import_exact(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::import_exact(module)?.into_query())
    }

    /// Import package pattern.
    pub fn import_package(module: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::import_package(module)?.into_query())
    }

    /// Static string reference.
    pub fn string_contains(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::string_contains(value)?.into_query())
    }

    /// Heuristic class reference.
    pub fn class_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::class_heuristic(name)?.into_query())
    }

    /// Module-export class reference.
    pub fn class_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::class_module(module, export)?.into_query())
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::constructor_global(name)?.into_query())
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(name: impl Into<String>) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::constructor_heuristic(name)?.into_query())
    }

    /// Module-export constructor.
    pub fn constructor_module(
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Result<Self, QueryBuildError> {
        Ok(EventQuery::constructor_module(module, export)?.into_query())
    }

    // ── Evidence override ─────────────────────────────────────────

    /// Override the evidence kind and symbol.
    #[must_use]
    pub fn with_evidence(mut self, kind: MatchKind, symbol: impl Into<String>) -> Self {
        self.emission.kind = kind;
        self.emission.symbol = symbol.into();
        self
    }

    /// Set the query expression.
    #[must_use]
    pub fn with_expression(mut self, expression: QueryExpr) -> Self {
        self.expression = expression;
        self
    }

    /// Set the primary evidence variable.
    #[must_use]
    pub fn with_primary_var(mut self, var: VarId) -> Self {
        self.emission.primary_var = var;
        self
    }

    /// Lower an [`ObjectFlowMatcher`] into a logical [`QueryDecl`] with a
    /// [`LifecycleQuery`] expression.
    ///
    /// Each source matcher becomes a source event query; the condition and
    /// completion are carried through as-is.  The emission kind is
    /// [`MatchKind::CallArgument`] and the symbol is the flow's symbol.
    ///
    /// # Panics
    ///
    /// Panics if the flow has no sources — callers must validate before
    /// calling. This method is deprecated; use [`QueryDecl::lifecycle`]
    /// instead.
    pub fn from_flow_matcher(flow: &ObjectFlowMatcher, _var_id: VarId) -> Self {
        let sources: Vec<EventQuery> = flow
            .sources()
            .iter()
            .map(|src| EventQuery {
                var: VarId::new(0),
                event: EventSpec::MemberCall {
                    member: SymbolPath::from(src.chain()),
                },
                identity: IdentitySpec::Rooted {
                    path: SymbolPath::from(src.chain()),
                },
                subject: SubjectSpec::Direct,
                constraints: src.arguments().to_vec(),
            })
            .collect();

        let lc = LifecycleQuery::new(
            sources,
            flow.condition().cloned(),
            flow.completion().cloned(),
        )
        .expect("from_flow_matcher: flow must have at least one source (caller must validate)");
        let expression = QueryExpr::Lifecycle(lc);
        let emission = EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::CallArgument,
            symbol: flow.symbol().to_owned(),
        };
        Self {
            expression,
            emission,
        }
    }
}

/// Sealed trait allowing [`RuleBuilder::query`] to accept either a
/// [`QueryDecl`] or a [`Result<QueryDecl, QueryBuildError>`] without
/// requiring the caller to unwrap.
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

mod private {
    pub trait Sealed {}
    impl Sealed for super::QueryDecl {}
    impl Sealed for Result<super::QueryDecl, super::QueryBuildError> {}
}

impl fmt::Display for QueryDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query emit={} {}", self.emission.symbol, self.expression)
    }
}

/// An ordered set of queries whose results are unioned.
///
/// A rule may have one or more independent queries. Each query produces its own
/// classified witnesses; the results are unioned across the set. This replaces
/// the implicit union of repeated `RuleBuilder::declaration` calls with an
/// explicit collection.
///
/// A `QuerySet` with a single query is the common case; multiple queries are
/// used when a rule matches several distinct event/identity combinations that
/// share metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuerySet {
    pub(crate) queries: Vec<QueryDecl>,
}

impl QuerySet {
    /// Create a query set from the given queries.
    pub fn new(queries: Vec<QueryDecl>) -> Self {
        Self { queries }
    }

    pub(crate) fn queries(&self) -> &[QueryDecl] {
        &self.queries
    }
}

impl fmt::Display for QuerySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QuerySet(")?;
        for (i, q) in self.queries.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{q}")?;
        }
        write!(f, ")")
    }
}

// ── Construction errors ──────────────────────────────────────────────

/// Errors from constructing or lowering logical query expressions.
///
/// These are declaration-layer errors, distinct from compiler validation
/// errors. They prevent construction of structurally invalid queries such as
/// empty `Any` or `All` expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryBuildError {
    /// An `Any` expression was constructed with zero branches.
    EmptyAlternatives,
    /// An `All` expression was constructed with zero branches.
    EmptyConjunction,
    /// A required name or identity string was empty.
    EmptyIdentityName,
    /// A module specifier string was empty.
    EmptyModuleSpecifier,
    /// A static value string was empty.
    EmptyStaticValue,
    /// A member chain is malformed (empty, double-dot, leading/trailing dot).
    MalformedChain(String),
    /// An argument index exceeds the maximum.
    InvalidArgumentIndex(usize),
    /// A package scope pattern is invalid.
    InvalidScopePackage,
    /// The number of constraints exceeds the limit.
    ExcessiveConstraints(usize),
    /// An alternative set or collection is empty when it must be non-empty.
    EmptyCollection(&'static str),
    /// A collection exceeds the maximum size.
    CollectionTooLarge(&'static str, usize),
}

impl fmt::Display for QueryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAlternatives => write!(f, "Any expression must have at least one branch"),
            Self::EmptyConjunction => write!(f, "All expression must have at least one branch"),
            Self::EmptyIdentityName => write!(f, "identity name must not be empty"),
            Self::EmptyModuleSpecifier => write!(f, "module specifier must not be empty"),
            Self::EmptyStaticValue => write!(f, "static value must not be empty"),
            Self::MalformedChain(chain) => write!(f, "malformed member chain: {chain}"),
            Self::InvalidArgumentIndex(idx) => {
                write!(
                    f,
                    "argument index {idx} exceeds maximum ({})",
                    limits::MAX_ARGUMENT_INDEX
                )
            }
            Self::InvalidScopePackage => write!(f, "invalid package scope pattern"),
            Self::ExcessiveConstraints(count) => {
                write!(f, "constraint count {count} exceeds limit")
            }
            Self::EmptyCollection(name) => write!(f, "{name} must not be empty"),
            Self::CollectionTooLarge(name, size) => {
                write!(f, "{name} size {size} exceeds maximum")
            }
        }
    }
}

/// Structured diagnostic projected from
/// [`crate::api::compiler::validate::QueryCompileError`] for public catalog
/// errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl QueryDiagnostic {
    #[allow(dead_code)]
    pub fn code(&self) -> &'static str {
        self.code
    }

    #[allow(dead_code)]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for QueryDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
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
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = AnyExpr::new(vec![event.clone(), event]).unwrap();
        assert_eq!(any.branches.len(), 2);
    }

    #[test]
    fn all_expr_accepts_non_empty_branches() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let all = AllExpr::new(vec![event]).unwrap();
        assert_eq!(all.branches.len(), 1);
    }

    // ── Construction: every convenience constructor → valid QueryDecl ──

    fn assert_event_query(decl: Result<QueryDecl, QueryBuildError>, expected_symbol: &str) {
        let decl = decl.unwrap();
        assert_eq!(decl.emission.primary_var, VarId::new(0));
        assert_eq!(decl.emission.symbol, expected_symbol);
        assert!(matches!(decl.expression, QueryExpr::Event(_)));
    }

    #[test]
    fn lowers_call_global_to_query_decl() {
        assert_event_query(QueryDecl::call_global("fetch"), "fetch");
    }

    #[test]
    fn lowers_call_heuristic_to_query_decl() {
        assert_event_query(QueryDecl::call_heuristic("fetch"), "fetch");
    }

    #[test]
    fn lowers_call_module_to_query_decl() {
        assert_event_query(QueryDecl::call_module("fs", "readFile"), "fs.readFile");
    }

    #[test]
    fn lowers_call_package_to_query_decl() {
        assert_event_query(
            QueryDecl::call_package("@scope/pkg", "method"),
            "@scope/pkg.method",
        );
    }

    #[test]
    fn lowers_member_call_rooted_to_query_decl() {
        assert_event_query(
            QueryDecl::member_call_rooted("document.createElement"),
            "document.createElement",
        );
    }

    #[test]
    fn lowers_member_call_heuristic_to_query_decl() {
        assert_event_query(QueryDecl::member_call_heuristic("foo.bar"), "foo.bar");
    }

    #[test]
    fn lowers_member_call_module_to_query_decl() {
        assert_event_query(QueryDecl::member_call_module("module", "method"), "module");
    }

    #[test]
    fn lowers_member_call_instance_to_query_decl() {
        assert_event_query(
            QueryDecl::member_call_instance("pkg", "Client", "send"),
            "pkg.Client",
        );
    }

    #[test]
    fn lowers_member_call_package_to_query_decl() {
        assert_event_query(
            QueryDecl::member_call_package("@scope/pkg", "method"),
            "@scope/pkg",
        );
    }

    #[test]
    fn lowers_member_call_returned_to_query_decl() {
        assert_event_query(QueryDecl::member_call_returned("create", "send"), "create");
    }

    #[test]
    fn lowers_member_read_rooted_to_query_decl() {
        assert_event_query(
            QueryDecl::member_read_rooted("window.location"),
            "window.location",
        );
    }

    #[test]
    fn lowers_member_read_module_to_query_decl() {
        assert_event_query(
            QueryDecl::member_read_module("module", "property"),
            "module",
        );
    }

    #[test]
    fn lowers_member_read_returned_to_query_decl() {
        assert_event_query(QueryDecl::member_read_returned("create", "token"), "create");
    }

    #[test]
    fn lowers_member_read_package_to_query_decl() {
        assert_event_query(
            QueryDecl::member_read_package("@scope/pkg", "property"),
            "@scope/pkg",
        );
    }

    #[test]
    fn lowers_import_exact_to_query_decl() {
        assert_event_query(QueryDecl::import_exact("node:fs"), "node:fs");
    }

    #[test]
    fn lowers_import_package_to_query_decl() {
        assert_event_query(QueryDecl::import_package("@scope/pkg"), "@scope/pkg");
    }

    #[test]
    fn lowers_string_contains_to_query_decl() {
        assert_event_query(QueryDecl::string_contains("https://"), "https://");
    }

    #[test]
    fn lowers_class_heuristic_to_query_decl() {
        assert_event_query(QueryDecl::class_heuristic("Worker"), "Worker");
    }

    #[test]
    fn lowers_class_module_to_query_decl() {
        assert_event_query(QueryDecl::class_module("module", "Klass"), "module.Klass");
    }

    #[test]
    fn lowers_constructor_global_to_query_decl() {
        assert_event_query(QueryDecl::constructor_global("URL"), "URL");
    }

    #[test]
    fn lowers_constructor_heuristic_to_query_decl() {
        assert_event_query(QueryDecl::constructor_heuristic("Foo"), "Foo");
    }

    #[test]
    fn lowers_constructor_module_to_query_decl() {
        assert_event_query(QueryDecl::constructor_module("pkg", "Klass"), "pkg.Klass");
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
        match &q.expression {
            QueryExpr::Event(eq) => {
                assert_eq!(eq.constraints.len(), 4);
            }
            _ => panic!("expected Event expression"),
        }
    }

    #[test]
    fn lowers_evidence_override_to_query_decl() {
        let q = QueryDecl::call_global("fetch")
            .unwrap()
            .with_evidence(MatchKind::CallArgument, "custom.fetch");
        assert_eq!(q.emission.kind, MatchKind::CallArgument);
        assert_eq!(q.emission.symbol, "custom.fetch");
    }

    // ── Equivalent forms produce equivalent declarations ──────────

    #[test]
    fn semantically_equivalent_decls_lower_equally() {
        let q_a = QueryDecl::call_global("fetch").unwrap();
        let q_b = QueryDecl::call_global("fetch").unwrap();
        assert_eq!(q_a, q_b);
    }

    // ── Diagnostic names ──────────────────────────────────────────

    #[test]
    fn query_expr_diagnostic_names_are_stable() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        assert_eq!(event.diagnostic_name(), "event");

        let any = QueryExpr::Any(AnyExpr::new(vec![event.clone()]).unwrap());
        assert_eq!(any.diagnostic_name(), "any");

        let all = QueryExpr::All(AllExpr::new(vec![event]).unwrap());
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

    #[test]
    fn subject_spec_diagnostic_names_are_stable() {
        assert_eq!(SubjectSpec::Direct.diagnostic_name(), "direct");
        assert_eq!(SubjectSpec::ReturnedFrom.diagnostic_name(), "returned_from");
        assert_eq!(SubjectSpec::InstanceOf.diagnostic_name(), "instance_of");
    }

    // ── Display and plan summary ──────────────────────────────────

    #[test]
    fn query_expr_display_shapes_are_compact() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let text = format!("{event}");
        assert!(text.contains("select"));
        assert!(text.contains("$0"));
        assert!(text.contains("call"));
        assert!(text.contains("global"));
        assert!(text.contains("direct"));
    }

    #[test]
    fn any_display_shows_branches() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = QueryExpr::Any(AnyExpr::new(vec![event]).unwrap());
        let text = format!("{any}");
        assert!(text.starts_with("any ["));
        assert!(text.ends_with(']'));
    }

    #[test]
    fn query_decl_display_includes_symbol() {
        let q = QueryDecl::call_global("fetch").unwrap();
        let text = format!("{q}");
        assert!(text.contains("fetch"));
    }

    #[test]
    fn queries_lower_correctly() {
        let queries = [
            QueryDecl::call_global("fetch").unwrap(),
            QueryDecl::member_read_rooted("window.location").unwrap(),
        ];
        assert_eq!(queries.len(), 2);
    }

    // ── VarId collection ──────────────────────────────────────────

    #[test]
    fn event_query_vars_contains_one() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(5),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        assert_eq!(event.vars(), vec![VarId::new(5)]);
    }

    #[test]
    fn any_query_vars_collects_all_branch_vars() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(1),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("g"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = QueryExpr::Any(AnyExpr::new(vec![a, b]).unwrap());
        let vars = any.vars();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&VarId::new(0)));
        assert!(vars.contains(&VarId::new(1)));
    }
}
