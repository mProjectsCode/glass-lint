//! The single compositional matcher declaration type and its validated builder.
//!
//! [`MatcherDecl`] is the only public matcher representation. It replaces all
//! parallel family-specific types (`CallMatcher`, `MemberCallMatcher`, etc.)
//! with one (identity, event, subject, constraints, evidence) model. The
//! builder rejects invalid combinations before compilation.

use smol_str::SmolStr;

use crate::{
    analysis::SymbolPath,
    api::{
        classification::MatchKind,
        compiler::{
            object_flow::CompiledObjectFlow,
            rule::{
                EventPredicate, EvidenceDescriptor, IdentityConstraint, IdentityStrength,
                QueryClause, QueryConstraint, SubjectConstraint,
            },
        },
        rule::{
            ArgumentConstraint, ArgumentMatcher, MatcherBuildError, ModuleSpecifierPattern,
            ValueMatcher, matcher::ObjectFlowMatcher,
        },
    },
};

/// One validated matcher declaration. Constructed exclusively through
/// [`MatcherDecl::builder`] or one of the convenience constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatcherDecl {
    pub(crate) identity: IdentityConstraint,
    pub(crate) event: EventPredicate,
    pub(crate) subject: SubjectConstraint,
    pub(crate) constraints: Vec<QueryConstraint>,
    pub(crate) evidence_kind: MatchKind,
    pub(crate) evidence_symbol: String,
    /// Compiled object flow when this is a flow lifecycle matcher.
    pub(crate) object_flow: Option<CompiledObjectFlow>,
}

impl MatcherDecl {
    /// Convert to an internal query clause for compilation.
    pub(crate) fn to_query_clause(&self) -> QueryClause {
        QueryClause {
            identity: self.identity.clone(),
            event: self.event.clone(),
            subject: self.subject.clone(),
            constraints: self.constraints.clone().into_boxed_slice(),
            evidence: EvidenceDescriptor {
                kind: self.evidence_kind,
                symbol: self.evidence_symbol.clone(),
            },
        }
    }

    /// Return a compiled object flow when this declaration carries flow info.
    /// Returns `None` for direct matchers; flow lifecycle declarations return
    /// the compiled flow.
    pub(crate) fn to_object_flow(&self) -> Option<CompiledObjectFlow> {
        self.object_flow.clone()
    }
}

impl MatcherDecl {
    /// Return a new declaration with an argument constraint appended.
    #[must_use]
    pub fn with_arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index, matcher,
            )));
        self
    }

    /// Return a new declaration with a static-string argument constraint.
    #[must_use]
    pub fn with_arg_static_string(mut self, index: usize) -> Self {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string(),
            )));
        self
    }

    /// Return a new declaration with static-string allowed values.
    #[must_use]
    pub fn with_arg_static_strings<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string().equals_any(values),
            )));
        self
    }

    #[must_use]
    pub fn with_arg_static_string_contains<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string().contains_any(values),
            )));
        self
    }

    #[must_use]
    pub fn with_arg_object_keys<I, S>(mut self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ArgumentMatcher::object_keys(keys),
            )));
        self
    }
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
    identity: Option<IdentityConstraint>,
    event: Option<EventPredicate>,
    subject: SubjectConstraint,
    constraints: Vec<QueryConstraint>,
    evidence_kind: Option<MatchKind>,
    evidence_symbol: Option<String>,
    validation_error: Option<MatcherBuildError>,
}

impl MatcherDeclBuilder {
    fn new() -> Self {
        Self {
            identity: None,
            event: None,
            subject: SubjectConstraint::Direct,
            constraints: Vec::new(),
            evidence_kind: None,
            evidence_symbol: None,
            validation_error: None,
        }
    }

    fn set_identity_event(
        &mut self,
        identity: IdentityConstraint,
        event: EventPredicate,
        symbol: String,
    ) {
        if self.identity.is_some() {
            self.validation_error = Some(MatcherBuildError::ConflictingProvenance);
            return;
        }
        self.evidence_kind = Some(match &event {
            EventPredicate::Call => MatchKind::Call,
            EventPredicate::Construct => MatchKind::Constructor,
            EventPredicate::MemberCall { .. } => MatchKind::MemberCall,
            EventPredicate::MemberRead { .. } => MatchKind::MemberRead,
            EventPredicate::ClassReference => MatchKind::Class,
            EventPredicate::Import => MatchKind::Import,
            EventPredicate::StringReference => MatchKind::StringContains,
        });
        self.evidence_symbol = Some(symbol);
        self.identity = Some(identity);
        self.event = Some(event);
    }

    /// Global call, e.g. `fetch(...)`.
    pub fn call_global(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        self.set_identity_event(
            IdentityConstraint::Global {
                name: name.clone(),
                strength: IdentityStrength::Strict,
            },
            EventPredicate::Call,
            name.to_string(),
        );
        self
    }

    /// Heuristic spelling call.
    pub fn call_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        self.set_identity_event(
            IdentityConstraint::Any {
                name: name.clone(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Call,
            name.to_string(),
        );
        self
    }

    /// Module-export call.
    pub fn call_module(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let export: SmolStr = export.into().into();
        let module: SmolStr = module.into().into();
        self.set_identity_event(
            IdentityConstraint::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventPredicate::Call,
            format!("{module}.{export}"),
        );
        self
    }

    pub fn call_package(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let export: SmolStr = export.into().into();
        let module = ModuleSpecifierPattern::package(module);
        let sym = module.to_string();
        self.set_identity_event(
            IdentityConstraint::PackageModuleExport {
                module,
                export: export.clone(),
            },
            EventPredicate::Call,
            format!("{sym}.{export}"),
        );
        self
    }

    /// Rooted member call, e.g. `document.createElement(...)`.
    pub fn member_call_rooted(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        let path = SymbolPath::from(chain_str.as_str());
        self.set_identity_event(
            IdentityConstraint::Rooted { path: path.clone() },
            EventPredicate::MemberCall { member: path },
            chain_str,
        );
        self
    }

    /// Heuristic member call.
    pub fn member_call_heuristic(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        let path = SymbolPath::from(chain_str.as_str());
        let name: SmolStr = chain_str.as_str().into();
        self.set_identity_event(
            IdentityConstraint::Any {
                name,
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::MemberCall { member: path },
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
        let path = SymbolPath::from(member_str.as_str());
        self.set_identity_event(
            IdentityConstraint::ModuleNamespace {
                module: module.clone(),
            },
            EventPredicate::MemberCall { member: path },
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
        let constructor = IdentityConstraint::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        };
        self.set_identity_event(
            constructor.clone(),
            EventPredicate::MemberCall {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{module}:{export}.{member}"),
        );
        self.subject = SubjectConstraint::InstanceOf {
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
        let path = SymbolPath::from(member_str.as_str());
        let module = ModuleSpecifierPattern::package(module);
        let sym = module.to_string();
        self.set_identity_event(
            IdentityConstraint::PackageModuleNamespace { module },
            EventPredicate::MemberCall { member: path },
            format!("{sym}.{member_str}"),
        );
        self
    }

    /// Rooted member read.
    pub fn member_read_rooted(mut self, chain: impl Into<String>) -> Self {
        let chain_str: String = chain.into();
        let path = SymbolPath::from(chain_str.as_str());
        self.set_identity_event(
            IdentityConstraint::Rooted { path: path.clone() },
            EventPredicate::MemberRead { member: path },
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
        let path = SymbolPath::from(member_str.as_str());
        self.set_identity_event(
            IdentityConstraint::ModuleNamespace {
                module: module.clone(),
            },
            EventPredicate::MemberRead { member: path },
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
        let producer = IdentityConstraint::Rooted {
            path: SymbolPath::from(source.as_str()),
        };
        self.set_identity_event(
            producer.clone(),
            EventPredicate::MemberCall {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{source}.{member}"),
        );
        self.subject = SubjectConstraint::ReturnedFrom {
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
        let producer = IdentityConstraint::Rooted {
            path: SymbolPath::from(source.as_str()),
        };
        self.set_identity_event(
            producer.clone(),
            EventPredicate::MemberRead {
                member: SymbolPath::from(member.as_str()),
            },
            format!("{source}.{member}"),
        );
        self.subject = SubjectConstraint::ReturnedFrom {
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
        let path = SymbolPath::from(member_str.as_str());
        let module = ModuleSpecifierPattern::package(module);
        let sym = module.to_string();
        self.set_identity_event(
            IdentityConstraint::PackageModuleNamespace { module },
            EventPredicate::MemberRead { member: path },
            format!("{sym}.{member_str}"),
        );
        self
    }

    /// Import exact module specifier.
    pub fn import_exact(mut self, module: impl Into<String>) -> Self {
        let module_str: String = module.into();
        self.set_identity_event(
            IdentityConstraint::LiteralString {
                predicate: module_str.clone(),
            },
            EventPredicate::Import,
            module_str,
        );
        self
    }

    /// Import package pattern.
    pub fn import_package(mut self, module: impl Into<String>) -> Self {
        let pattern = ModuleSpecifierPattern::package(module);
        let sym = pattern.to_string();
        self.identity = Some(IdentityConstraint::PackageSpecifier { pattern });
        self.event = Some(EventPredicate::Import);
        self.evidence_kind = Some(MatchKind::Import);
        self.evidence_symbol = Some(sym);
        self
    }

    /// Static string reference.
    pub fn string_contains(mut self, value: impl Into<String>) -> Self {
        let value_str: String = value.into();
        self.set_identity_event(
            IdentityConstraint::LiteralString {
                predicate: value_str.clone(),
            },
            EventPredicate::StringReference,
            value_str,
        );
        self
    }

    /// Heuristic class reference.
    pub fn class_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        self.set_identity_event(
            IdentityConstraint::Any {
                name: name.clone(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::ClassReference,
            name.to_string(),
        );
        self
    }

    /// Module-export class reference.
    pub fn class_module(mut self, module: impl Into<String>, export: impl Into<String>) -> Self {
        let module: SmolStr = module.into().into();
        let export: SmolStr = export.into().into();
        self.set_identity_event(
            IdentityConstraint::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventPredicate::ClassReference,
            format!("{module}.{export}"),
        );
        self
    }

    /// Global constructor, e.g. `new URL(...)`.
    pub fn constructor_global(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        self.set_identity_event(
            IdentityConstraint::Global {
                name: name.clone(),
                strength: IdentityStrength::Strict,
            },
            EventPredicate::Construct,
            name.to_string(),
        );
        self
    }

    /// Heuristic constructor.
    pub fn constructor_heuristic(mut self, name: impl Into<String>) -> Self {
        let name: SmolStr = name.into().into();
        self.set_identity_event(
            IdentityConstraint::Any {
                name: name.clone(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Construct,
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
        self.set_identity_event(
            IdentityConstraint::ModuleExport {
                module: module.clone(),
                export: export.clone(),
            },
            EventPredicate::Construct,
            format!("{module}.{export}"),
        );
        self
    }

    /// Set the subject to [`SubjectConstraint::ReturnedFrom`].
    pub fn returned_from(mut self, producer: IdentityConstraint) -> Self {
        self.subject = SubjectConstraint::ReturnedFrom {
            producer: Box::new(producer),
        };
        self
    }

    /// Set the subject to [`SubjectConstraint::InstanceOf`].
    pub fn instance_of(mut self, constructor: IdentityConstraint) -> Self {
        self.subject = SubjectConstraint::InstanceOf {
            constructor: Box::new(constructor),
        };
        self
    }

    /// Add an argument predicate.
    pub fn arg(mut self, index: usize, matcher: impl Into<ArgumentMatcher>) -> Self {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index, matcher,
            )));
        self
    }

    /// Add a static-string argument constraint.
    pub fn arg_static_string(mut self, index: usize) -> Self {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string(),
            )));
        self
    }

    /// Add a static-string constraint with allowed values.
    pub fn arg_static_strings<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string().equals_any(values),
            )));
        self
    }

    pub fn arg_static_string_contains<I, S>(mut self, index: usize, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ValueMatcher::static_string().contains_any(values),
            )));
        self
    }

    pub fn arg_object_property_value(
        mut self,
        index: usize,
        property: impl Into<String>,
        value: ValueMatcher,
    ) -> Self {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ArgumentMatcher::object_property_value(property, value),
            )));
        self
    }

    pub fn arg_object_keys<I, S>(mut self, index: usize, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.constraints
            .push(QueryConstraint::Argument(ArgumentConstraint::new(
                index,
                ArgumentMatcher::object_keys(keys),
            )));
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
        let identity = self
            .identity
            .ok_or_else(|| MatcherBuildError::Generic("missing identity constraint".into()))?;
        let event = self
            .event
            .ok_or_else(|| MatcherBuildError::Generic("missing event predicate".into()))?;
        let evidence_kind = self.evidence_kind.unwrap_or(MatchKind::Call);
        let evidence_symbol = self
            .evidence_symbol
            .unwrap_or_else(|| format!("{identity:?}"));
        // Basic validation
        let constraints = self.constraints;
        if !constraints.is_empty()
            && !matches!(
                event,
                EventPredicate::Call | EventPredicate::MemberCall { .. }
            )
        {
            return Err(MatcherBuildError::Generic(
                "argument constraints require a call event".into(),
            ));
        }
        // Validate argument index bounds
        for c in &constraints {
            let QueryConstraint::Argument(a) = c;
            if a.index() > 1_000_000 {
                return Err(MatcherBuildError::InvalidArgumentIndex(a.index()));
            }
        }
        Ok(MatcherDecl {
            identity,
            event,
            subject: self.subject,
            constraints,
            evidence_kind,
            evidence_symbol,
            object_flow: None,
        })
    }
}

impl MatcherDecl {
    /// Create a declaration from an old-style ObjectFlowMatcher.
    pub fn from_object_flow(flow: &ObjectFlowMatcher) -> Self {
        let sym: String = flow.symbol().to_owned();
        Self {
            identity: IdentityConstraint::Any {
                name: smol_str::SmolStr::new(sym.as_str()),
                strength: crate::api::compiler::rule::IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            subject: SubjectConstraint::Direct,
            constraints: Vec::new(),
            evidence_kind: MatchKind::Call,
            evidence_symbol: sym,
            object_flow: Some(CompiledObjectFlow::from_matcher(flow)),
        }
    }
}
