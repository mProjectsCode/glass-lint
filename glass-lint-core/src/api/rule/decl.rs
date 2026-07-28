//! The single compositional matcher declaration type and its validated builder.
//!
//! [`MatcherDecl`] is the only public matcher representation. It replaces all
//! parallel family-specific types (`CallMatcher`, `MemberCallMatcher`, etc.)
//! with one (identity, event, subject, constraints, evidence) model. The
//! builder rejects invalid combinations before compilation.

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ArgumentConstraint, ArgumentMatcher, MatcherBuildError, ModuleSpecifierPattern,
        ValueMatcher,
        query::{EventSpec, IdentitySpec, SubjectSpec},
    },
};

/// One validated matcher declaration. Constructed exclusively through
/// [`MatcherDecl::builder`].
///
/// This type represents ordinary (clause-based) matching. Object flow matching
/// uses [`crate::api::rule::ObjectFlowMatcher`] directly through
/// [`RuleBuilder::object_flow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatcherDecl {
    pub(crate) identity: IdentitySpec,
    pub(crate) event: EventSpec,
    pub(crate) subject: SubjectSpec,
    pub(crate) constraints: Vec<ArgumentConstraint>,
    pub(crate) evidence_kind: MatchKind,
    pub(crate) evidence_symbol: String,
}

// ── Builder entry ──

impl MatcherDecl {
    pub fn builder() -> MatcherDeclBuilder {
        MatcherDeclBuilder::new()
    }
}

// ── Builder ───────────────────────────────────────────────────────────────

/// Validated builder for a single [`MatcherDecl`].
///
/// Call exactly one identity/event method (e.g. [`call_global`]) to set the
/// core dimensions, then optionally attach argument constraints, subject
/// modifiers, and evidence metadata before calling [`build`].
///
/// [`call_global`]: MatcherDeclBuilder::call_global
/// [`build`]: MatcherDeclBuilder::build
#[derive(Debug)]
pub struct MatcherDeclBuilder {
    identity: Option<IdentitySpec>,
    event: Option<EventSpec>,
    subject: SubjectSpec,
    constraints: Vec<ArgumentConstraint>,
    evidence_kind: Option<MatchKind>,
    evidence_symbol: Option<String>,
    validation_error: Option<MatcherBuildError>,
}

impl MatcherDeclBuilder {
    fn new() -> Self {
        Self {
            identity: None,
            event: None,
            subject: SubjectSpec::Direct,
            constraints: Vec::new(),
            evidence_kind: None,
            evidence_symbol: None,
            validation_error: None,
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

    fn set_identity_event(&mut self, identity: IdentitySpec, event: EventSpec, symbol: String) {
        if self.identity.is_some() {
            self.validation_error = Some(MatcherBuildError::ConflictingProvenance);
            return;
        }
        self.evidence_kind = Some(Self::evidence_kind_for_event(&event));
        self.evidence_symbol = Some(symbol);
        self.identity = Some(identity);
        self.event = Some(event);
    }

    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::Global { name: name.clone() },
            EventSpec::Call,
            name.to_string(),
        );
        self
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::Heuristic { name: name.clone() },
            EventSpec::Call,
            name.to_string(),
        );
        self
    }

    /// Module-export call.
    pub fn call_module(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() || export.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventSpec::Call,
            format!("{module}.{export}"),
        );
        self
    }

    pub fn call_package(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let export: SmolStr = export.into().into();
        match ModuleSpecifierPattern::package(module) {
            Ok(module) => {
                let sym = module.to_string();
                self.set_identity_event(
                    IdentitySpec::PackageModuleExport {
                        module,
                        export: export.clone(),
                    },
                    EventSpec::Call,
                    format!("{sym}.{export}"),
                );
            }
            Err(e) => self.validation_error = Some(e),
        }
        self
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        if Self::is_chain_malformed(&chain_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(chain_str.as_str());
        self.set_identity_event(
            IdentitySpec::Rooted { path: path.clone() },
            EventSpec::MemberCall { member: path },
            chain_str,
        );
        self
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        if Self::is_chain_malformed(&chain_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(chain_str.as_str());
        let name: SmolStr = chain_str.as_str().into();
        self.set_identity_event(
            IdentitySpec::Heuristic { name },
            EventSpec::MemberCall { member: path },
            chain_str,
        );
        self
    }

    /// Module-namespace member call.
    pub fn member_call_module(
        mut self,
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let module: SmolStr = module.into().into();
        let member_str: String = member.into();
        if module.trim().is_empty() || Self::is_chain_malformed(&member_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(member_str.as_str());
        self.set_identity_event(
            IdentitySpec::ModuleNamespace {
                module: module.clone(),
            },
            EventSpec::MemberCall { member: path },
            format!("{module}.{member_str}"),
        );
        self
    }

    /// Member call on an instance created by a module export.
    pub fn member_call_instance(
        mut self,
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        let member: SmolStr = member.into().into();
        if module.trim().is_empty() || export.trim().is_empty() || Self::is_chain_malformed(&member)
        {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let constructor = IdentitySpec::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        };
        self.set_identity_event(
            constructor.clone(),
            EventSpec::MemberCall {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{module}:{export}.{member}"),
        );
        self.subject = SubjectSpec::InstanceOf {
            constructor: Box::new(constructor),
        };
        self
    }

    pub fn member_call_package(
        mut self,
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let member_str: String = member.into();
        if Self::is_chain_malformed(&member_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(member_str.as_str());
        match ModuleSpecifierPattern::package(module) {
            Ok(module) => {
                let sym = module.to_string();
                self.set_identity_event(
                    IdentitySpec::PackageModuleNamespace { module },
                    EventSpec::MemberCall { member: path },
                    format!("{sym}.{member_str}"),
                );
            }
            Err(e) => self.validation_error = Some(e),
        }
        self
    }

    /// Rooted member read.
    pub fn member_read_rooted(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        if Self::is_chain_malformed(&chain_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(chain_str.as_str());
        self.set_identity_event(
            IdentitySpec::Rooted { path: path.clone() },
            EventSpec::MemberRead { member: path },
            chain_str,
        );
        self
    }

    /// Module-namespace member read.
    pub fn member_read_module(
        mut self,
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let module: SmolStr = module.into().into();
        let member_str: String = member.into();
        if module.trim().is_empty() || Self::is_chain_malformed(&member_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(member_str.as_str());
        self.set_identity_event(
            IdentitySpec::ModuleNamespace {
                module: module.clone(),
            },
            EventSpec::MemberRead { member: path },
            format!("{module}.{member_str}"),
        );
        self
    }

    /// Member call on an object returned by a rooted source.
    pub fn member_call_returned(
        mut self,
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let source = source.into();
        let member: SmolStr = member.into().into();
        if Self::is_chain_malformed(&source) || Self::is_chain_malformed(&member) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let producer = IdentitySpec::Rooted {
            path: SymbolPath::from(source.as_str()),
        };
        self.set_identity_event(
            producer.clone(),
            EventSpec::MemberCall {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{source}.{member}"),
        );
        self.subject = SubjectSpec::ReturnedFrom {
            producer: Box::new(producer),
        };
        self
    }

    /// Member read on an object returned by a rooted source.
    pub fn member_read_returned(
        mut self,
        source: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let source = source.into();
        let member: SmolStr = member.into().into();
        if Self::is_chain_malformed(&source) || Self::is_chain_malformed(&member) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let producer = IdentitySpec::Rooted {
            path: SymbolPath::from(source.as_str()),
        };
        self.set_identity_event(
            producer.clone(),
            EventSpec::MemberRead {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{source}.{member}"),
        );
        self.subject = SubjectSpec::ReturnedFrom {
            producer: Box::new(producer),
        };
        self
    }

    pub fn member_read_package(
        mut self,
        module: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        let member_str: String = member.into();
        if Self::is_chain_malformed(&member_str) {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        let path = SymbolPath::from(member_str.as_str());
        match ModuleSpecifierPattern::package(module) {
            Ok(module) => {
                let sym = module.to_string();
                self.set_identity_event(
                    IdentitySpec::PackageModuleNamespace { module },
                    EventSpec::MemberRead { member: path },
                    format!("{sym}.{member_str}"),
                );
            }
            Err(e) => self.validation_error = Some(e),
        }
        self
    }

    /// Import exact module specifier.
    pub fn import_exact(mut self, module: impl Into<String>) -> Self {
        let module_str: String = module.into();
        if module_str.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::LiteralString {
                predicate: module_str.clone(),
            },
            EventSpec::Import,
            module_str,
        );
        self
    }

    /// Import package pattern.
    pub fn import_package(mut self, module: impl Into<String>) -> Self {
        match ModuleSpecifierPattern::package(module) {
            Ok(pattern) => {
                let sym = pattern.to_string();
                self.identity = Some(IdentitySpec::PackageSpecifier { pattern });
                self.event = Some(EventSpec::Import);
                self.evidence_kind = Some(MatchKind::Import);
                self.evidence_symbol = Some(sym);
            }
            Err(e) => self.validation_error = Some(e),
        }
        self
    }

    /// Static string reference.
    pub fn string_contains(mut self, value: impl Into<String>) -> Self {
        let value_str: String = value.into();
        if value_str.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::LiteralString {
                predicate: value_str.clone(),
            },
            EventSpec::StringReference,
            value_str,
        );
        self
    }

    /// Heuristic class reference.
    pub fn class_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::Heuristic { name: name.clone() },
            EventSpec::ClassReference,
            name.to_string(),
        );
        self
    }

    /// Module-export class reference.
    pub fn class_module(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() || export.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventSpec::ClassReference,
            format!("{module}.{export}"),
        );
        self
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::Global { name: name.clone() },
            EventSpec::Construct,
            name.to_string(),
        );
        self
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        if name.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::Heuristic { name: name.clone() },
            EventSpec::Construct,
            name.to_string(),
        );
        self
    }

    /// Module-export constructor.
    pub fn constructor_module(
        mut self,
        module: impl Into<String>,
        export: impl Into<String>,
    ) -> Self {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        if module.trim().is_empty() || export.trim().is_empty() {
            self.validation_error = Some(MatcherBuildError::EmptyChain);
            return self;
        }
        self.set_identity_event(
            IdentitySpec::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventSpec::Construct,
            format!("{module}.{export}"),
        );
        self
    }

    /// Add an argument predicate.
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.constraints
            .push(ArgumentConstraint::new(index, matcher));
        self
    }

    /// Add a static-string argument constraint.
    pub fn arg_static_string(mut self, index: usize) -> Self {
        self.constraints.push(ArgumentConstraint::new(
            index,
            ValueMatcher::static_string(),
        ));
        self
    }

    /// Add a static-string constraint with allowed values.
    pub fn arg_static_strings<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints.push(ArgumentConstraint::new(
            index,
            ValueMatcher::static_string().equals_any(values),
        ));
        self
    }

    pub fn arg_static_string_contains<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints.push(ArgumentConstraint::new(
            index,
            ValueMatcher::static_string().contains_any(values),
        ));
        self
    }

    pub fn arg_object_property_value(
        mut self,
        index: usize,
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Self {
        self.constraints.push(ArgumentConstraint::new(
            index,
            ArgumentMatcher::object_property_value(property, value),
        ));
        self
    }

    pub fn arg_object_keys<I, S>(mut self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints.push(ArgumentConstraint::new(
            index,
            ArgumentMatcher::object_keys(keys),
        ));
        self
    }

    /// Override the default evidence kind.
    pub fn evidence(mut self, kind: MatchKind, symbol: impl Into<String>) -> Self {
        self.evidence_kind = Some(kind);
        self.evidence_symbol = Some(symbol.into());
        self
    }

    /// Validate and build the declaration.
    pub fn build(self) -> Result<MatcherDecl, MatcherBuildError> {
        if let Some(error) = self.validation_error {
            return Err(error);
        }
        let identity = self.identity.ok_or(MatcherBuildError::MissingRequired)?;
        let event = self.event.ok_or(MatcherBuildError::MissingRequired)?;
        let evidence_kind = self.evidence_kind.unwrap_or(MatchKind::Call);
        let evidence_symbol = self
            .evidence_symbol
            .unwrap_or_else(|| identity.display_name());
        let constraints = self.constraints;
        if !constraints.is_empty()
            && !matches!(event, EventSpec::Call | EventSpec::MemberCall { .. })
        {
            return Err(MatcherBuildError::ConstraintsOnNonCallEvent);
        }
        // Validate argument index bounds
        for c in &constraints {
            if c.index() > 1_000_000 {
                return Err(MatcherBuildError::InvalidArgumentIndex(c.index()));
            }
        }
        Ok(MatcherDecl {
            identity,
            event,
            subject: self.subject,
            constraints,
            evidence_kind,
            evidence_symbol,
        })
    }
}
