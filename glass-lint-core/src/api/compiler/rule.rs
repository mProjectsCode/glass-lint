//! Compiled declarative matcher plans and object-flow projections.
//!
//! The compiler preserves matcher semantics in owned, immutable structures.
//! Selection only filters catalog indexes; it never changes the semantic facts
//! constructed for a source file.

use std::fmt;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::{
    Severity,
    analysis::matches_global_object_alias,
    api::{
        classification::{MatchKind, RuleIndex},
        compiler::{
            normalize,
            object_flow::CompiledObjectFlow,
            validate::{validate_normalized_decl, validate_query_decl},
        },
        rule::{
            ArgumentConstraint, Confidence, MatcherBuildError, MatcherDecl, ModuleSpecifierPattern,
            query::{EventSpec, IdentitySpec, QueryDecl, SubjectSpec, VarId},
        },
    },
};

/// Canonical compiled matcher plan consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
///
/// This is the sole compiled-plan type.  Consumers access clauses and flows
/// through accessors; there is no separate plan wrapper.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    clauses: Box<[QueryClause]>,
    flows: Box<[CompiledObjectFlow]>,
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
    /// Return true when the identity references an empty name or predicate.
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Any { name, .. } | Self::Global { name, .. } => name.is_empty(),
            Self::ModuleExport { module, export } => module.is_empty() || export.is_empty(),
            Self::PackageModuleExport { module, export } => {
                module.as_str().is_empty() || export.is_empty()
            }
            Self::ModuleNamespace { module } => module.is_empty(),
            Self::PackageModuleNamespace { module } => module.as_str().is_empty(),
            Self::Rooted { path } => path.is_empty(),
            Self::LiteralString { predicate } => predicate.is_empty(),
            Self::PackageSpecifier { pattern } => pattern.as_str().is_empty(),
        }
    }

    pub(crate) fn root_or_descendant_matches(
        &self,
        source: &SymbolPath,
        environment: &crate::Environment,
    ) -> bool {
        matches!(self, Self::Rooted { path } if matches_global_object_alias(path, source, environment)
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

// ── Lowering: declaration types → compiler IR ────────────────────────────

fn lower_identity(spec: &IdentitySpec) -> IdentityConstraint {
    match spec {
        IdentitySpec::Global { name } => IdentityConstraint::Global {
            name: name.clone(),
            strength: IdentityStrength::Strict,
        },
        IdentitySpec::Heuristic { name } => IdentityConstraint::Any {
            name: name.clone(),
            strength: IdentityStrength::Heuristic,
        },
        IdentitySpec::ModuleExport { module, export } => IdentityConstraint::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        },
        IdentitySpec::PackageModuleExport { module, export } => {
            IdentityConstraint::PackageModuleExport {
                module: module.clone(),
                export: export.clone(),
            }
        }
        IdentitySpec::ModuleNamespace { module } => IdentityConstraint::ModuleNamespace {
            module: module.clone(),
        },
        IdentitySpec::PackageModuleNamespace { module } => {
            IdentityConstraint::PackageModuleNamespace {
                module: module.clone(),
            }
        }
        IdentitySpec::Rooted { path } => IdentityConstraint::Rooted { path: path.clone() },
        IdentitySpec::LiteralString { predicate } => IdentityConstraint::LiteralString {
            predicate: predicate.clone(),
        },
        IdentitySpec::PackageSpecifier { pattern } => IdentityConstraint::PackageSpecifier {
            pattern: pattern.clone(),
        },
    }
}

fn lower_event(spec: &EventSpec) -> EventPredicate {
    match spec {
        EventSpec::Call => EventPredicate::Call,
        EventSpec::Construct => EventPredicate::Construct,
        EventSpec::MemberCall { member } => EventPredicate::MemberCall {
            member: member.clone(),
        },
        EventSpec::MemberRead { member } => EventPredicate::MemberRead {
            member: member.clone(),
        },
        EventSpec::ClassReference => EventPredicate::ClassReference,
        EventSpec::Import => EventPredicate::Import,
        EventSpec::StringReference => EventPredicate::StringReference,
    }
}

fn lower_subject(spec: &SubjectSpec) -> SubjectConstraint {
    match spec {
        SubjectSpec::Direct => SubjectConstraint::Direct,
        SubjectSpec::ReturnedFrom { producer } => SubjectConstraint::ReturnedFrom {
            producer: Box::new(lower_identity(producer)),
        },
        SubjectSpec::InstanceOf { constructor } => SubjectConstraint::InstanceOf {
            constructor: Box::new(lower_identity(constructor)),
        },
    }
}

fn lower_to_clause(decl: &MatcherDecl) -> QueryClause {
    QueryClause {
        identity: lower_identity(&decl.identity),
        event: lower_event(&decl.event),
        subject: lower_subject(&decl.subject),
        constraints: decl
            .constraints
            .iter()
            .cloned()
            .map(QueryConstraint::Argument)
            .collect(),
        evidence: EvidenceDescriptor {
            kind: decl.evidence_kind,
            symbol: decl.evidence_symbol.clone(),
        },
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum InvalidQueryClause {
    /// The identity/event/subject dimensions cannot select a semantic fact.
    ImpossibleDimensions,
    /// Argument predicates require a call-bearing event.
    ConstraintsRequireCallEvent,
}

impl fmt::Display for InvalidQueryClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImpossibleDimensions => {
                f.write_str("identity/event/subject dimensions cannot select a semantic fact")
            }
            Self::ConstraintsRequireCallEvent => {
                f.write_str("argument constraints require a call-bearing event")
            }
        }
    }
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
        if self.identity.is_empty() {
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

/// Shared declaration compilation: convert each declaration to a logical
/// query, validate it, lower to a clause, then sort, deduplicate, and
/// validate every clause.
fn collect_clauses(decls: &[MatcherDecl]) -> Result<Vec<QueryClause>, MatcherBuildError> {
    // Phase 4: Validate each declaration as a logical QueryDecl.
    for (i, decl) in decls.iter().enumerate() {
        let var_id = VarId::new(u32::try_from(i).unwrap_or(u32::MAX));
        let query = QueryDecl::from_matcher(decl, var_id);
        validate_query_decl(&query).map_err(|e| {
            MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
        })?;

        // Phase 5: Normalize the logical query into canonical form.
        let (normalized, _requirements) = normalize::normalize_query_decl(&query);
        validate_normalized_decl(&normalized).map_err(|e| {
            MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
        })?;
        // The normalized form and plan requirements are reserved for
        // Phase 6 (physical planning).  Lowering still uses the original
        // MatcherDecl for now.
    }

    let mut clauses: Vec<QueryClause> = Vec::new();
    for decl in decls {
        clauses.push(lower_to_clause(decl));
    }
    clauses.sort();
    clauses.dedup();
    for clause in &clauses {
        clause
            .validate()
            .map_err(|error| MatcherBuildError::InvalidLoweredQuery(error.to_string()))?;
    }
    Ok(clauses)
}

impl CompiledMatcherPlan {
    pub(crate) fn clauses(&self) -> &[QueryClause] {
        &self.clauses
    }

    pub(crate) fn flows(&self) -> &[CompiledObjectFlow] {
        &self.flows
    }

    /// Compile declarations into clauses.
    /// Used by test helpers.
    #[cfg(test)]
    pub(crate) fn compile_decls(decls: &[MatcherDecl]) -> Result<Self, MatcherBuildError> {
        let clauses = collect_clauses(decls)?;
        Ok(Self {
            clauses: clauses.into_boxed_slice(),
            flows: Box::new([]),
        })
    }

    /// Compile declarations and object flows into a complete plan.
    pub(crate) fn compile_decls_and_flows(
        decls: &[MatcherDecl],
        flows: &[crate::api::rule::ObjectFlowMatcher],
    ) -> Result<Self, MatcherBuildError> {
        let clauses = collect_clauses(decls)?;
        let compiled_flows: Vec<CompiledObjectFlow> = flows
            .iter()
            .map(|flow| {
                let compiled = CompiledObjectFlow::from_matcher(flow);
                if compiled.symbol.trim().is_empty() {
                    return Err(MatcherBuildError::EmptyFlowSymbol);
                }
                if compiled.sources.is_empty() {
                    return Err(MatcherBuildError::EmptyFlowSources);
                }
                if compiled.requirements.is_empty() && !compiled.all_requirements_required {
                    return Err(MatcherBuildError::MissingFlowCondition);
                }
                if compiled.sinks.is_empty() && !compiled.emit_on_requirements {
                    return Err(MatcherBuildError::MissingFlowCompletion);
                }
                Ok(compiled)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            clauses: clauses.into_boxed_slice(),
            flows: compiled_flows.into_boxed_slice(),
        })
    }
}

#[derive(Debug, Clone)]
/// Borrowed view of compiled rules selected for a classification run.
pub(crate) struct CompiledRuleSelection<'a> {
    /// All compiled rules, retained for stable rule indexes.
    pub(crate) rules: &'a [CompiledRuleRecord],
    /// Sorted selected rule indexes.
    pub(crate) selected: &'a [RuleIndex],
}

impl<'a> CompiledRuleSelection<'a> {
    /// Create a borrowed catalog view over sorted selected indexes.
    pub fn new(rules: &'a [CompiledRuleRecord], selected: &'a [RuleIndex]) -> Self {
        Self { rules, selected }
    }

    /// Iterate selected plans while preserving their catalog indexes.
    pub fn selected_matchers(&self) -> impl Iterator<Item = (RuleIndex, &CompiledMatcherPlan)> {
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
    pub fn get(&self, index: RuleIndex) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index.get()).map(|rule| &rule.matcher)
    }

    /// Return the total catalog rule count.
    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

#[derive(Debug, Clone)]
/// Immutable compiled rule record containing metadata and the query plan.
/// Retains no source declaration tree after construction.
pub(crate) struct CompiledRuleRecord {
    /// Human-readable description.
    pub(crate) description: String,
    /// Report severity.
    pub(crate) severity: Severity,
    /// Evidence confidence.
    pub(crate) confidence: Confidence,
    /// Compiled query plan.
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRuleRecord {
    /// Compile a rule's declarations and flows into one record.
    pub(crate) fn new(rule: &crate::api::rule::Rule) -> Result<Self, MatcherBuildError> {
        let plan = CompiledMatcherPlan::compile_decls_and_flows(
            rule.declarations(),
            rule.flow_matchers(),
        )?;
        Ok(Self {
            description: rule.description().to_owned(),
            severity: rule.severity(),
            confidence: rule.confidence(),
            matcher: plan,
        })
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::SymbolPath;

    use super::*;
    use crate::api::{
        classification::MatchKind,
        rule::{MatcherDecl, ValueMatcher},
    };

    #[test]
    fn every_declaration_compiles_into_one_plan() {
        let decls = vec![
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_call_rooted("window.open")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_read_rooted("window.location")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .import_exact("node:fs")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .import_package("@scope/pkg")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .string_contains("https://")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .class_heuristic("Worker")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .constructor_global("URL")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_call_returned("create", "send")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_read_returned("create", "token")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_call_instance("pkg", "Client", "send")
                .build()
                .expect("valid matcher declaration"),
        ];
        let plan = CompiledMatcherPlan::compile_decls(&decls).unwrap();
        assert!(!plan.clauses().is_empty());
    }

    #[test]
    fn argument_matcher_compiles_to_one_query_clause() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .evidence(MatchKind::CallArgument, "fetch")
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile_decls(&[decl]).unwrap();
        let clauses = plan.clauses();
        assert_eq!(clauses.len(), 1);
        assert!(!clauses[0].constraints.is_empty());
        assert_eq!(clauses[0].evidence.kind, MatchKind::CallArgument);
    }

    #[test]
    fn invalid_declarations_return_a_compile_error() {
        // Missing identity + event should cause a build error
        let decl = MatcherDecl::builder()
            .evidence(MatchKind::Call, "test")
            .build();
        assert!(decl.is_err());
    }

    #[test]
    fn equivalent_declarations_compile_to_identical_queries() {
        let first = vec![
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_read_rooted("location.href")
                .build()
                .expect("valid matcher declaration"),
        ];
        let second = vec![
            MatcherDecl::builder()
                .member_read_rooted("location.href")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        ];

        let first = CompiledMatcherPlan::compile_decls(&first).unwrap();
        let second = CompiledMatcherPlan::compile_decls(&second).unwrap();
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }

    #[test]
    fn query_plan_compiles_declarations_into_composable_dimensions() {
        let decls = vec![
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_call_rooted("window.open")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_read_returned("create", "token")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_call_instance("pkg", "Client", "send")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .import_exact("node:fs")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .string_contains("https://")
                .build()
                .expect("valid matcher declaration"),
        ];
        let plan = CompiledMatcherPlan::compile_decls(&decls).unwrap();
        let clauses = plan.clauses();
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
        let first = vec![
            MatcherDecl::builder()
                .call_heuristic("fetch")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .member_read_rooted("location.href")
                .build()
                .expect("valid matcher declaration"),
        ];
        let second = vec![
            MatcherDecl::builder()
                .member_read_rooted("location.href")
                .build()
                .expect("valid matcher declaration"),
            MatcherDecl::builder()
                .call_heuristic("fetch")
                .build()
                .expect("valid matcher declaration"),
        ];
        let first = CompiledMatcherPlan::compile_decls(&first).unwrap();
        let second = CompiledMatcherPlan::compile_decls(&second).unwrap();
        assert_eq!(first.clauses(), second.clauses());
        assert_eq!(first.clauses(), first.clauses());
    }

    #[test]
    fn decl_with_argument_constraint_keeps_call_kind() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .evidence(MatchKind::CallArgument, "fetch")
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile_decls(&[decl]).unwrap();
        let clauses = plan.clauses();
        assert_eq!(clauses.len(), 1);
        for left in clauses {
            for right in clauses {
                assert_eq!(left == right, left.cmp(right).is_eq());
            }
        }
    }
}
