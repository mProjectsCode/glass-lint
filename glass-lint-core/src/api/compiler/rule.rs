//! Compiled declarative matcher plans and object-flow projections.
//!
//! The compiler preserves matcher semantics in owned, immutable structures.
//! Selection only filters catalog indexes; it never changes the semantic facts
//! constructed for a source file.

use smol_str::{SmolStr, ToSmolStr};

use super::super::{
    classification::MatchKind, rule::{
        ArgumentConstraint, ClassMatcher, ConstructorMatcher, FlowCompletion, FlowCondition, FlowSinkMatcher, InstanceMemberCallMatcher, MatcherFamily, MatcherSet, MemberCallMatcher, MemberCallProvenance, MemberReadMatcher, MemberReadProvenance, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, Rule, SymbolProvenance, ValueMatcher,
    },
};
use crate::{analysis::SymbolPath, api::{classification::RuleIndex, rule::ModuleSpecifierPattern}};

/// Canonical matcher representation consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    query: QueryPlan,
}

#[derive(Debug, Clone)]
/// Private compositional query representation consumed by semantic analysis.
pub(crate) struct QueryPlan {
    pub(crate) clauses: Box<[QueryClause]>,
    pub(crate) flows: Box<[CompiledObjectFlow]>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct QueryClause {
    pub(crate) identity: IdentityConstraint,
    pub(crate) event: EventPredicate,
    pub(crate) subject: SubjectConstraint,
    pub(crate) constraints: Box<[QueryConstraint]>,
    pub(crate) evidence: EvidenceDescriptor,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum IdentityStrength {
    Strict,
    Heuristic,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum IdentityConstraint {
    Any {
        name: SmolStr,
        strength: IdentityStrength,
    },
    Global {
        name: SmolStr,
        strength: IdentityStrength,
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
    /// Free-form substring predicate retained intentionally for literal
    /// matching; unlike identities, it is not an API symbol.
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
}

impl IdentityConstraint {
    pub(crate) fn root_or_descendant_matches(
        &self,
        source: &SymbolPath,
        environment: &crate::Environment,
    ) -> bool {
        matches!(self, Self::Rooted { path } if path.matches_global_object_alias(source, environment)
            || source.is_equal_or_descendant_of(path))
    }

    pub(crate) fn exact_root_matches(&self, source: &SymbolPath) -> bool {
        matches!(self, Self::Rooted { path } if path == source)
    }

    pub(crate) fn identity_module_matches(&self, module: &str, export: &str) -> bool {
        matches!(self, Self::ModuleExport { module: expected_module, export: expected_export } if expected_module == module && expected_export == export)
            || matches!(self, Self::PackageModuleExport { module: expected_module, export: expected_export } if expected_module.matches(module) && expected_export == export)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum EventPredicate {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum SubjectConstraint {
    Direct,
    ReturnedFrom {
        producer: Box<IdentityConstraint>,
    },
    InstanceOf {
        constructor: Box<IdentityConstraint>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum QueryConstraint {
    Argument(ArgumentConstraint),
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct EvidenceDescriptor {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum InvalidQueryClause {
    /// The identity/event/subject dimensions cannot select a semantic fact.
    ImpossibleDimensions,
    /// Argument predicates require a call-bearing event.
    ConstraintsRequireCallEvent,
}

impl QueryClause {
    pub(crate) fn validate(&self) -> Result<(), InvalidQueryClause> {
        let dimensions_are_valid = matches!(
            (&self.identity, &self.event, &self.subject),
            (
                IdentityConstraint::Any { .. }
                    | IdentityConstraint::Global { .. }
                    | IdentityConstraint::ModuleExport { .. }
                    | IdentityConstraint::PackageModuleExport { .. },
                EventPredicate::Call | EventPredicate::Construct,
                SubjectConstraint::Direct,
            ) | (
                IdentityConstraint::Any { .. }
                    | IdentityConstraint::Rooted { .. }
                    | IdentityConstraint::ModuleNamespace { .. }
                    | IdentityConstraint::PackageModuleNamespace { .. },
                EventPredicate::MemberCall { .. } | EventPredicate::MemberRead { .. },
                SubjectConstraint::Direct,
            ) | (
                IdentityConstraint::Any { .. }
                    | IdentityConstraint::ModuleExport { .. }
                    | IdentityConstraint::PackageModuleExport { .. },
                EventPredicate::ClassReference,
                SubjectConstraint::Direct,
            ) | (
                IdentityConstraint::LiteralString { .. }
                    | IdentityConstraint::PackageSpecifier { .. },
                EventPredicate::Import | EventPredicate::StringReference,
                SubjectConstraint::Direct,
            ) | (
                IdentityConstraint::Rooted { .. },
                EventPredicate::MemberCall { .. } | EventPredicate::MemberRead { .. },
                SubjectConstraint::ReturnedFrom { .. },
            ) | (
                IdentityConstraint::ModuleExport { .. }
                    | IdentityConstraint::PackageModuleExport { .. },
                EventPredicate::MemberCall { .. },
                SubjectConstraint::InstanceOf { .. },
            )
        );
        if !dimensions_are_valid {
            return Err(InvalidQueryClause::ImpossibleDimensions);
        }
        let subject_identity_is_valid = match &self.subject {
            SubjectConstraint::Direct => match (&self.identity, &self.event) {
                (
                    IdentityConstraint::Any { name, .. },
                    EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member },
                ) => member.eq_chain(name),
                (
                    IdentityConstraint::Rooted { path },
                    EventPredicate::MemberCall { member } | EventPredicate::MemberRead { member },
                ) => path == member,
                _ => true,
            },
            SubjectConstraint::ReturnedFrom { producer } => producer.as_ref() == &self.identity,
            SubjectConstraint::InstanceOf { constructor } => constructor.as_ref() == &self.identity,
        };
        if !subject_identity_is_valid {
            return Err(InvalidQueryClause::ImpossibleDimensions);
        }
        if !self.constraints.is_empty()
            && !matches!(
                self.event,
                EventPredicate::Call | EventPredicate::MemberCall { .. }
            )
        {
            return Err(InvalidQueryClause::ConstraintsRequireCallEvent);
        }
        Ok(())
    }
}

impl QueryPlan {
    fn from_matcher(matcher: &MatcherSet, flows: Vec<CompiledObjectFlow>) -> Self {
        let mut clauses = Vec::new();
        for family in matcher.families() {
            match family {
                MatcherFamily::Calls(values) => clauses.extend(lower_calls(values)),
                MatcherFamily::MemberCalls(values) => clauses.extend(lower_member_calls(values)),
                MatcherFamily::MemberReads(values) => clauses.extend(lower_member_reads(values)),
                MatcherFamily::Imports(values) => clauses.extend(lower_literals(values, &[], &[])),
                MatcherFamily::PackageImports(values) => {
                    clauses.extend(lower_literals(&[], values, &[]));
                }
                MatcherFamily::StringContains(values) => {
                    clauses.extend(lower_literals(&[], &[], values));
                }
                MatcherFamily::Classes(values) => {
                    clauses.extend(lower_classes_and_constructors(values, &[]));
                }
                MatcherFamily::Constructors(values) => {
                    clauses.extend(lower_classes_and_constructors(&[], values));
                }
                MatcherFamily::Flows(values) => {
                    let _ = values;
                }
                MatcherFamily::ReturnedMemberCalls(values) => {
                    clauses.extend(lower_returned_members(values, &[]));
                }
                MatcherFamily::ReturnedMemberReads(values) => {
                    clauses.extend(lower_returned_members(&[], values));
                }
                MatcherFamily::InstanceMemberCalls(values) => {
                    clauses.extend(lower_instance_members(values));
                }
            }
        }
        clauses.sort();
        clauses.dedup();
        for clause in &clauses {
            clause
                .validate()
                .expect("matcher compiler produced an invalid query clause");
        }
        Self {
            clauses: clauses.into_boxed_slice(),
            flows: flows.into_boxed_slice(),
        }
    }

    pub(crate) fn clauses(&self) -> &[QueryClause] {
        &self.clauses
    }

    pub(crate) fn flows(&self) -> &[CompiledObjectFlow] {
        &self.flows
    }
}

fn lower_calls(values: &[super::super::rule::CallMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|call| QueryClause {
            identity: call_identity(&call.name, &call.provenance),
            event: EventPredicate::Call,
            subject: SubjectConstraint::Direct,
            constraints: call
                .arguments
                .iter()
                .cloned()
                .map(QueryConstraint::Argument)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            evidence: EvidenceDescriptor {
                kind: if call.arguments.is_empty() {
                    MatchKind::Call
                } else {
                    MatchKind::CallArgument
                },
                symbol: call.evidence_symbol(),
            },
        })
        .collect()
}

fn lower_member_calls(values: &[MemberCallMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|member| QueryClause {
            identity: member_identity(&member.chain, &member.provenance),
            event: EventPredicate::MemberCall {
                member: SymbolPath::from(member.chain.as_str()),
            },
            subject: SubjectConstraint::Direct,
            constraints: member
                .arguments
                .iter()
                .cloned()
                .map(QueryConstraint::Argument)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            evidence: EvidenceDescriptor {
                kind: if member.arguments.is_empty() {
                    MatchKind::MemberCall
                } else {
                    MatchKind::CallArgument
                },
                symbol: member.evidence_symbol(),
            },
        })
        .collect()
}

fn lower_member_reads(values: &[MemberReadMatcher]) -> Vec<QueryClause> {
    values
        .iter()
        .map(|read| QueryClause {
            identity: member_read_identity(&read.chain, &read.provenance),
            event: EventPredicate::MemberRead {
                member: SymbolPath::from(read.chain.as_str()),
            },
            subject: SubjectConstraint::Direct,
            constraints: Box::new([]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::MemberRead,
                symbol: read.evidence_symbol(),
            },
        })
        .collect()
}

fn lower_literals(
    imports: &[String],
    package_imports: &[ModuleSpecifierPattern],
    string_contains: &[String],
) -> Vec<QueryClause> {
    let imports = imports
        .iter()
        .map(|value| literal_clause(value, EventPredicate::Import, MatchKind::Import));
    let packages = package_imports.iter().map(|pattern| QueryClause {
        identity: IdentityConstraint::PackageSpecifier {
            pattern: pattern.clone(),
        },
        event: EventPredicate::Import,
        subject: SubjectConstraint::Direct,
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::Import,
            symbol: pattern.to_string(),
        },
    });
    let strings = string_contains.iter().map(|value| {
        literal_clause(
            value,
            EventPredicate::StringReference,
            MatchKind::StringContains,
        )
    });
    imports.chain(packages).chain(strings).collect()
}

fn literal_clause(value: &str, event: EventPredicate, kind: MatchKind) -> QueryClause {
    QueryClause {
        identity: IdentityConstraint::LiteralString {
            predicate: value.to_owned(),
        },
        event,
        subject: SubjectConstraint::Direct,
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind,
            symbol: value.to_owned(),
        },
    }
}

fn lower_classes_and_constructors(
    classes: &[ClassMatcher],
    constructors: &[ConstructorMatcher],
) -> Vec<QueryClause> {
    let classes = classes.iter().map(|class| QueryClause {
        identity: call_identity(&class.name, &class.provenance),
        event: EventPredicate::ClassReference,
        subject: SubjectConstraint::Direct,
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::Class,
            symbol: class.evidence_symbol(),
        },
    });
    let constructors = constructors.iter().map(|constructor| QueryClause {
        identity: call_identity(&constructor.name, &constructor.provenance),
        event: EventPredicate::Construct,
        subject: SubjectConstraint::Direct,
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::Constructor,
            symbol: constructor.evidence_symbol(),
        },
    });
    classes.chain(constructors).collect()
}

fn lower_returned_members(
    calls_matchers: &[ReturnedMemberCallMatcher],
    reads_matchers: &[ReturnedMemberReadMatcher],
) -> Vec<QueryClause> {
    let calls = calls_matchers.iter().map(|returned| {
        returned_member_clause(
            &returned.source,
            &returned.member,
            EventPredicate::MemberCall {
                member: returned.member.clone().into(),
            },
            MatchKind::MemberCall,
        )
    });
    let reads = reads_matchers.iter().map(|returned| {
        returned_member_clause(
            &returned.source,
            &returned.member,
            EventPredicate::MemberRead {
                member: returned.member.clone().into(),
            },
            MatchKind::MemberRead,
        )
    });
    calls.chain(reads).collect()
}

fn returned_member_clause(
    source: &str,
    member: &str,
    event: EventPredicate,
    kind: MatchKind,
) -> QueryClause {
    QueryClause {
        identity: IdentityConstraint::Rooted {
            path: SymbolPath::from(source),
        },
        event,
        subject: SubjectConstraint::ReturnedFrom {
            producer: Box::new(IdentityConstraint::Rooted {
                path: SymbolPath::from(source),
            }),
        },
        constraints: Box::new([]),
        evidence: EvidenceDescriptor {
            kind,
            symbol: format!("{source}.{member}"),
        },
    }
}

fn lower_instance_members(
    values: &[InstanceMemberCallMatcher],
) -> Vec<QueryClause> {
    values
        .iter()
        .map(|instance| {
            let constructor = instance.module_pattern.clone().map_or_else(
                || IdentityConstraint::ModuleExport {
                    module: instance.module.to_smolstr(),
                    export: instance.export.to_smolstr(),
                },
                |module| IdentityConstraint::PackageModuleExport {
                    module,
                    export: instance.export.to_smolstr(),
                },
            );
            QueryClause {
                identity: constructor.clone(),
                event: EventPredicate::MemberCall {
                    member: SymbolPath::from(instance.member.as_str()),
                },
                subject: SubjectConstraint::InstanceOf {
                    constructor: Box::new(constructor),
                },
                constraints: Box::new([]),
                evidence: EvidenceDescriptor {
                    kind: MatchKind::MemberCall,
                    symbol: format!(
                        "{}:{}.{}",
                        instance.module, instance.export, instance.member
                    ),
                },
            }
        })
        .collect()
}

fn member_identity(
    chain: &str,
    provenance: &MemberCallProvenance,
) -> IdentityConstraint {
    match provenance {
        MemberCallProvenance::Any => IdentityConstraint::Any {
            name: chain.to_smolstr(),
            strength: IdentityStrength::Heuristic,
        },
        MemberCallProvenance::Rooted => IdentityConstraint::Rooted {
            path: SymbolPath::from(chain),
        },
        MemberCallProvenance::ModuleNamespace { module } => {
            IdentityConstraint::ModuleNamespace {
                module: module.to_smolstr(),
            }
        }
        MemberCallProvenance::PackageModuleNamespace { module } => {
            IdentityConstraint::PackageModuleNamespace {
                module: module.clone(),
            }
        }
    }
}

fn member_read_identity(
    chain: &str,
    provenance: &MemberReadProvenance,
) -> IdentityConstraint {
    match provenance {
        MemberReadProvenance::Any => IdentityConstraint::Any {
            name: chain.to_smolstr(),
            strength: IdentityStrength::Heuristic,
        },
        MemberReadProvenance::Rooted => IdentityConstraint::Rooted {
            path: SymbolPath::from(chain),
        },
        MemberReadProvenance::ModuleNamespace { module } => {
            IdentityConstraint::ModuleNamespace {
                module: module.to_smolstr(),
            }
        }
        MemberReadProvenance::PackageModuleNamespace { module } => {
            IdentityConstraint::PackageModuleNamespace {
                module: module.clone(),
            }
        }
    }
}

fn call_identity(
    name: &str,
    provenance: &SymbolProvenance,
) -> IdentityConstraint {
    match provenance {
        SymbolProvenance::Any => IdentityConstraint::Any {
            name: name.into(),
            strength: IdentityStrength::Heuristic,
        },
        SymbolProvenance::Global => IdentityConstraint::Global {
            name: name.into(),
            strength: IdentityStrength::Strict,
        },
        SymbolProvenance::ModuleExport { module } => {
            IdentityConstraint::ModuleExport {
                module: module.to_smolstr(),
                export: name.into(),
            }
        }
        SymbolProvenance::PackageModuleExport { module } => {
            IdentityConstraint::PackageModuleExport {
                module: module.clone(),
                export: name.into(),
            }
        }
    }
}

#[derive(Debug, Clone)]
/// Borrowed view of compiled rules selected for a classification run.
pub(crate) struct CompiledRuleSelection<'a> {
    /// All compiled rules, retained for stable rule indexes.
    pub(crate) rules: &'a [CompiledRule],
    /// Sorted selected rule indexes.
    pub(crate) selected: &'a [RuleIndex],
}

#[derive(Debug, Clone)]
/// Compiled source/requirement/sink flow configuration for one symbol.
pub(crate) struct CompiledObjectFlow {
    /// Evidence symbol emitted for this flow.
    pub(crate) symbol: String,
    /// Object-producing member-call sources.
    pub(crate) sources: Vec<CompiledObjectSource>,
    /// Required object events.
    pub(crate) requirements: Vec<CompiledObjectRequirement>,
    /// Terminal sink patterns.
    pub(crate) sinks: Vec<CompiledObjectSink>,
    /// Whether every configured requirement must be observed.
    pub(crate) all_requirements_required: bool,
    /// Whether configuration itself emits evidence after requirements.
    pub(crate) emit_on_requirements: bool,
}

impl CompiledObjectFlow {
    /// Return the flow's stable evidence symbol.
    pub fn evidence_symbol(&self) -> String {
        self.symbol.clone()
    }

    /// Test a sink chain, provenance mode, and argument position.
    pub fn sink_matches(&self, chain: Option<&SymbolPath>, rooted: bool, argument: usize) -> bool {
        self.sinks.iter().any(|sink| {
            sink.member_calls.iter().any(|member| chain == Some(member))
                && sink.provenance.matches_rooted(rooted)
                && match &sink.args {
                    CompiledObjectSinkArguments::Any => true,
                    CompiledObjectSinkArguments::Indices(indices) => indices.contains(&argument),
                }
        })
    }

    /// Whether the observed requirement count satisfies this flow condition.
    pub fn requirements_ready(&self, completed: usize) -> bool {
        if self.all_requirements_required {
            completed == self.requirements.len()
        } else {
            completed != 0
        }
    }

    /// Compile one public object-flow matcher into owned plan data.
    pub fn from_matcher(flow: &ObjectFlowMatcher) -> Self {
        let (requirements, all_requirements_required) = match flow.condition.as_ref() {
            Some(FlowCondition::AnyOf(events)) => (
                events
                    .iter()
                    .map(CompiledObjectRequirement::from_matcher)
                    .collect(),
                false,
            ),
            Some(FlowCondition::AllOf(events)) => (
                events
                    .iter()
                    .map(CompiledObjectRequirement::from_matcher)
                    .collect(),
                true,
            ),
            None => (Vec::new(), false),
        };
        let (sinks, emit_on_requirements) = match flow.completion.as_ref() {
            Some(FlowCompletion::Configuration) => (Vec::new(), true),
            Some(FlowCompletion::AnySink(sinks)) => (
                sinks.iter().map(CompiledObjectSink::from_matcher).collect(),
                false,
            ),
            None => (Vec::new(), false),
        };
        Self {
            symbol: flow.symbol.clone(),
            sources: flow
                .sources
                .iter()
                .map(CompiledObjectSource::from_matcher)
                .collect(),
            requirements,
            sinks,
            all_requirements_required,
            emit_on_requirements,
        }
    }
}

#[derive(Debug, Clone)]
/// Compiled member-call source constraint.
pub(crate) struct CompiledObjectSource {
    /// Required member-call chain.
    pub(crate) member_call: SymbolPath,
    /// Argument constraints on the source call.
    pub(crate) arguments: Vec<ArgumentConstraint>,
    /// Required rooted/module provenance mode.
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSource {
    fn from_matcher(source: &ObjectSourceMatcher) -> Self {
        Self {
            member_call: SymbolPath::from(source.call.chain()),
            arguments: source.call.arguments().to_vec(),
            provenance: source.call.provenance.clone(),
        }
    }
}

#[derive(Debug, Clone)]
/// Event that must be observed on a flowed object.
pub(crate) enum CompiledObjectRequirement {
    /// Required property write and value constraint.
    PropertyWrite {
        /// Written property name.
        property: SmolStr,
        /// Required value matcher.
        value: ValueMatcher,
    },
    /// Required member call and argument constraints.
    MemberCall {
        /// Required member-call name.
        member: SymbolPath,
        /// Argument constraints for the call.
        arguments: Vec<ArgumentConstraint>,
    },
}

impl CompiledObjectRequirement {
    fn from_matcher(event: &ObjectEventMatcher) -> Self {
        match event {
            ObjectEventMatcher::PropertyWrite { property, value } => Self::PropertyWrite {
                property: property.clone(),
                value: value.clone(),
            },
            ObjectEventMatcher::MemberCall { member, arguments } => Self::MemberCall {
                member: SymbolPath::from(member.as_str()),
                arguments: arguments.clone(),
            },
        }
    }
}

#[derive(Debug, Clone)]
/// Argument-position matching mode for a compiled sink.
pub(crate) enum CompiledObjectSinkArguments {
    /// Match every argument position present at the call site.
    Any,
    /// Match only configured argument positions.
    Indices(Vec<usize>),
}

impl CompiledObjectSinkArguments {
    /// Return only sink argument positions that exist at this call site.
    ///
    /// Keeping the bounds check here makes callers unable to accidentally
    /// treat a rule's configured index as proof that the argument was passed.
    pub fn present_indices(&self, argument_count: usize) -> Vec<usize> {
        match self {
            Self::Any => (0..argument_count).collect(),
            Self::Indices(indices) => indices
                .iter()
                .copied()
                .filter(|index| *index < argument_count)
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
/// Compiled terminal sink pattern for object flow.
pub(crate) struct CompiledObjectSink {
    /// Accepted sink member-call chains.
    pub(crate) member_calls: Vec<SymbolPath>,
    /// Accepted argument-position mode.
    pub(crate) args: CompiledObjectSinkArguments,
    /// Required rooted/module provenance mode.
    pub(crate) provenance: MemberCallProvenance,
}

impl CompiledObjectSink {
    fn from_matcher(sink: &FlowSinkMatcher) -> Self {
        match sink {
            FlowSinkMatcher::ArgumentOf { call, index } => Self {
                member_calls: vec![SymbolPath::from(call.chain())],
                args: CompiledObjectSinkArguments::Indices(vec![*index]),
                provenance: call.provenance.clone(),
            },
            FlowSinkMatcher::AnyArgumentOf { call } => Self {
                member_calls: vec![SymbolPath::from(call.chain())],
                args: CompiledObjectSinkArguments::Any,
                provenance: call.provenance.clone(),
            },
        }
    }
}

impl CompiledMatcherPlan {
    /// Compile a public API matcher and all of its object flows.
    pub fn compile(matcher: &MatcherSet) -> Self {
        let flows = matcher
            .families()
            .into_iter()
            .find_map(|family| match family {
                MatcherFamily::Flows(values) => Some(values),
                _ => None,
            })
            .unwrap_or_default()
            .iter()
            .map(CompiledObjectFlow::from_matcher)
            .collect();
        Self {
            query: QueryPlan::from_matcher(matcher, flows),
        }
    }

    /// Borrow the normalized query used by all semantic execution paths.
    pub(crate) fn query(&self) -> &QueryPlan {
        &self.query
    }
}

impl<'a> CompiledRuleSelection<'a> {
    /// Create a borrowed catalog view over sorted selected indexes.
    pub fn new(
        rules: &'a [CompiledRule],
        selected: &'a [RuleIndex],
    ) -> Self {
        Self { rules, selected }
    }

    /// Iterate selected plans while preserving their catalog indexes.
    pub fn selected_matchers(
        &self,
    ) -> impl Iterator<Item = (RuleIndex, &CompiledMatcherPlan)> {
        self.selected.iter().filter_map(move |&index| {
            self.rules
                .get(index.get())
                .map(|rule| (index, &rule.matcher))
        })
    }

    /// Whether a catalog index is selected by this view.
    pub fn is_selected(&self, index: RuleIndex) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    /// Borrow a compiled plan by its stable catalog index.
    pub fn get(
        &self,
        index: RuleIndex,
    ) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index.get()).map(|rule| &rule.matcher)
    }

    /// Return the total catalog rule count.
    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

#[derive(Debug, Clone)]
/// One public rule paired with its compiled matcher plan.
pub(crate) struct CompiledRule {
    /// Compiled matcher data for the rule.
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRule {
    /// Compile a rule's declared matcher list into one canonical plan.
    pub fn new(rule: &Rule) -> Self {
        Self {
            matcher: CompiledMatcherPlan::compile(&MatcherSet::from_matchers(
                rule.matchers().to_vec(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompiledMatcherPlan, EventPredicate, IdentityConstraint, IdentityStrength,
        SubjectConstraint,
    };
    use crate::{
        analysis::SymbolPath,
        api::{
            classification::MatchKind,
            rule::{CallMatcher, Matcher, MatcherSet, ObjectFlowMatcher},
        },
    };

    #[test]
    fn argument_matcher_compiles_to_one_query_clause() {
        let matcher = MatcherSet::from_matchers(vec![Matcher::from(
            CallMatcher::global("fetch").arg_static_strings(0, ["/api"]),
        )]);
        let plan = CompiledMatcherPlan::compile(&matcher);
        let clauses = plan.query().clauses();
        assert_eq!(clauses.len(), 1);
        assert_eq!(clauses[0].constraints.len(), 1);
        assert_eq!(clauses[0].evidence.kind, MatchKind::CallArgument);
    }

    #[test]
    fn compiles_every_public_matcher_family_into_one_query() {
        let matcher = MatcherSet::from_matchers(vec![
            Matcher::heuristic_call("fetch"),
            Matcher::rooted_member_call("window.open"),
            Matcher::rooted_member_read("window.location"),
            Matcher::import("node:fs"),
            Matcher::string_contains("https://"),
            Matcher::heuristic_class("Worker"),
            Matcher::global_constructor("URL"),
            Matcher::from(ObjectFlowMatcher {
                symbol: "request".into(),
                sources: Vec::new(),
                condition: None,
                completion: None,
            }),
            Matcher::returned_member_call("create", "send"),
            Matcher::returned_member_read("create", "token"),
            Matcher::instance_member_call("pkg", "Client", "send"),
        ]);

        let plan = CompiledMatcherPlan::compile(&matcher);
        let query = plan.query();
        assert_eq!(query.clauses.len(), 10);
        assert_eq!(query.flows.len(), 1);
    }

    #[test]
    fn equivalent_declarations_compile_to_identical_queries() {
        let first = MatcherSet::from_matchers(vec![
            Matcher::global_call("fetch"),
            Matcher::rooted_member_read("location.href"),
        ]);
        let second = MatcherSet::from_matchers(vec![
            Matcher::rooted_member_read("location.href"),
            Matcher::global_call("fetch"),
        ]);

        assert_eq!(
            format!("{:?}", CompiledMatcherPlan::compile(&first).query()),
            format!("{:?}", CompiledMatcherPlan::compile(&second).query())
        );
    }

    #[test]
    fn query_plan_compiles_public_families_into_composable_dimensions() {
        let matcher = MatcherSet::from_matchers(vec![
            Matcher::global_call("fetch"),
            Matcher::rooted_member_call("window.open"),
            Matcher::returned_member_read("create", "token"),
            Matcher::instance_member_call("pkg", "Client", "send"),
            Matcher::import("node:fs"),
            Matcher::string_contains("https://"),
        ]);
        let plan = CompiledMatcherPlan::compile(&matcher);
        let clauses = plan.query().clauses();
        assert!(clauses.iter().any(|clause| matches!(
            (&clause.identity, &clause.event, &clause.subject),
            (IdentityConstraint::Global { name, strength: IdentityStrength::Strict }, EventPredicate::Call, SubjectConstraint::Direct) if name == "fetch"
        )));
        assert!(clauses.iter().any(|clause| matches!(
            (&clause.identity, &clause.event),
            (IdentityConstraint::Rooted { path }, EventPredicate::MemberCall { member }) if *path == SymbolPath::from("window.open") && member.eq_chain("window.open")
        )));
        assert!(clauses.iter().any(|clause| matches!(
            (&clause.subject, &clause.event),
            (SubjectConstraint::ReturnedFrom { .. }, EventPredicate::MemberRead { member }) if member.eq_chain("token")
        )));
        assert!(clauses.iter().any(|clause| matches!(
            (&clause.subject, &clause.event),
            (SubjectConstraint::InstanceOf { .. }, EventPredicate::MemberCall { member }) if member.eq_chain("send")
        )));
        assert!(
            clauses
                .iter()
                .any(|clause| matches!(clause.event, EventPredicate::Import))
        );
        assert!(
            clauses
                .iter()
                .any(|clause| matches!(clause.event, EventPredicate::StringReference))
        );
    }

    #[test]
    fn query_plan_normalization_is_idempotent_and_order_independent() {
        let first = MatcherSet::from_matchers(vec![
            Matcher::heuristic_call("fetch"),
            Matcher::rooted_member_read("location.href"),
        ]);
        let second = MatcherSet::from_matchers(vec![
            Matcher::rooted_member_read("location.href"),
            Matcher::heuristic_call("fetch"),
        ]);
        let first = CompiledMatcherPlan::compile(&first);
        let second = CompiledMatcherPlan::compile(&second);
        assert_eq!(first.query().clauses(), second.query().clauses());
        assert_eq!(first.query().clauses(), first.query().clauses());
    }

    #[test]
    fn query_clause_eq_and_ord_are_consistent() {
        let matcher = MatcherSet::from_matchers(vec![
            Matcher::from(CallMatcher::global("fetch").arg_static_strings(0, ["/api"])),
            Matcher::global_call("fetch"),
        ]);
        let plan = CompiledMatcherPlan::compile(&matcher);
        let clauses = plan.query().clauses();
        assert_eq!(clauses.len(), 2);
        for left in clauses {
            for right in clauses {
                assert_eq!(left == right, left.cmp(right).is_eq());
            }
        }
        assert_ne!(clauses[0].evidence.kind, clauses[1].evidence.kind);
        assert_ne!(clauses[0].cmp(&clauses[1]), std::cmp::Ordering::Equal);
    }
}
