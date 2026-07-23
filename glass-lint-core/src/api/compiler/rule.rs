//! Compiled declarative matcher plans and object-flow projections.
//!
//! The compiler preserves matcher semantics in owned, immutable structures.
//! Selection only filters catalog indexes; it never changes the semantic facts
//! constructed for a source file.

use smol_str::SmolStr;

use crate::{
    analysis::SymbolPath,
    api::{
        classification::{MatchKind, RuleIndex},
        compiler::object_flow::CompiledObjectFlow,
        rule::{
            ArgumentConstraint, Category, Confidence, MatcherBuildError, MatcherDecl,
            ModuleSpecifierPattern,
        },
    },
};

/// Canonical matcher representation consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
#[derive(Debug, Clone)]
pub struct CompiledMatcherPlan {
    query: QueryPlan,
}

#[derive(Debug, Clone)]
/// Private compositional query representation consumed by semantic analysis.
pub struct QueryPlan {
    pub(crate) clauses: Box<[QueryClause]>,
    pub(crate) flows: Box<[CompiledObjectFlow]>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct QueryClause {
    pub(crate) identity: IdentityConstraint,
    pub(crate) event: EventPredicate,
    pub(crate) subject: SubjectConstraint,
    pub(crate) constraints: Box<[QueryConstraint]>,
    pub(crate) evidence: EvidenceDescriptor,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum IdentityStrength {
    Strict,
    Heuristic,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum IdentityConstraint {
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
pub enum EventPredicate {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum SubjectConstraint {
    Direct,
    ReturnedFrom {
        producer: Box<IdentityConstraint>,
    },
    InstanceOf {
        constructor: Box<IdentityConstraint>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum QueryConstraint {
    Argument(ArgumentConstraint),
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvidenceDescriptor {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidQueryClause {
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

impl QueryPlan {
    pub(crate) fn clauses(&self) -> &[QueryClause] {
        &self.clauses
    }

    pub(crate) fn flows(&self) -> &[CompiledObjectFlow] {
        &self.flows
    }

    #[cfg(test)]
    fn from_declarations(decls: &[MatcherDecl]) -> Result<Self, MatcherBuildError> {
        let mut clauses = Vec::new();
        let mut flows = Vec::new();
        for decl in decls {
            let clause = decl.to_query_clause();
            clauses.push(clause);
            if let Some(matcher) = &decl.object_flow {
                flows.push(CompiledObjectFlow::from_matcher(matcher));
            }
        }
        clauses.sort();
        clauses.dedup();
        for clause in &clauses {
            clause.validate().map_err(|error| {
                MatcherBuildError::Generic(format!("invalid lowered matcher query: {error:?}"))
            })?;
        }
        Ok(Self {
            clauses: clauses.into_boxed_slice(),
            flows: flows.into_boxed_slice(),
        })
    }
}

impl CompiledMatcherPlan {
    #[cfg(test)]
    pub(crate) fn compile(decls: &[MatcherDecl]) -> Result<Self, MatcherBuildError> {
        let query = QueryPlan::from_declarations(decls)?;
        Ok(Self { query })
    }

    /// Compile declarations into clauses and extract flows.
    pub(crate) fn compile_decls(decls: &[MatcherDecl]) -> Result<Self, MatcherBuildError> {
        let mut clauses: Vec<QueryClause> = Vec::new();
        let mut flows: Vec<CompiledObjectFlow> = Vec::new();
        for decl in decls {
            clauses.push(decl.to_query_clause());
            if let Some(matcher) = &decl.object_flow {
                flows.push(CompiledObjectFlow::from_matcher(matcher));
            }
        }
        clauses.sort();
        clauses.dedup();
        for clause in &clauses {
            clause.validate().map_err(|error| {
                MatcherBuildError::Generic(format!("invalid lowered matcher query: {error:?}"))
            })?;
        }
        for clause in &clauses {
            if let IdentityConstraint::PackageSpecifier { pattern }
            | IdentityConstraint::PackageModuleExport {
                module: pattern, ..
            }
            | IdentityConstraint::PackageModuleNamespace { module: pattern } = &clause.identity
            {
                pattern.validate().map_err(|e| {
                    MatcherBuildError::Generic(format!("invalid package specifier: {e}"))
                })?;
            }
        }
        for flow in &flows {
            if flow.symbol.trim().is_empty() {
                return Err(MatcherBuildError::Generic(
                    "object flow symbol must not be empty".into(),
                ));
            }
            if flow.sources.is_empty() {
                return Err(MatcherBuildError::Generic(
                    "object flow must have at least one source".into(),
                ));
            }
            if flow.requirements.is_empty() && !flow.all_requirements_required {
                return Err(MatcherBuildError::Generic(
                    "object flow must have a condition".into(),
                ));
            }
            if flow.sinks.is_empty() && !flow.emit_on_requirements {
                return Err(MatcherBuildError::Generic(
                    "object flow must have a completion mode".into(),
                ));
            }
        }
        Ok(Self {
            query: QueryPlan {
                clauses: clauses.into_boxed_slice(),
                flows: flows.into_boxed_slice(),
            },
        })
    }

    /// Borrow the normalized query used by all semantic execution paths.
    pub(crate) fn query(&self) -> &QueryPlan {
        &self.query
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
pub struct CompiledRuleRecord {
    /// Provider-local rule name (before namespace prefix).
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Provider-defined category.
    pub category: Category,
    /// Report severity.
    pub severity: crate::Severity,
    /// Evidence confidence.
    pub confidence: Confidence,
    /// Compiled query plan.
    pub matcher: CompiledMatcherPlan,
}

impl CompiledRuleRecord {
    /// Compile a rule's declarations into one record.
    pub(crate) fn new(rule: &crate::api::rule::Rule) -> Result<Self, MatcherBuildError> {
        let plan = CompiledMatcherPlan::compile_decls(rule.declarations())?;
        Ok(Self {
            id: rule.id().to_owned(),
            description: rule.description().to_owned(),
            category: rule.category().clone(),
            severity: rule.severity(),
            confidence: rule.confidence(),
            matcher: plan,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis::SymbolPath,
        api::{
            classification::MatchKind,
            rule::{MatcherDecl, ValueMatcher},
        },
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
        let plan = CompiledMatcherPlan::compile(&decls).unwrap();
        assert!(!plan.query().clauses().is_empty());
    }

    #[test]
    fn argument_matcher_compiles_to_one_query_clause() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .evidence(MatchKind::CallArgument, "fetch")
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile(&[decl]).unwrap();
        let clauses = plan.query().clauses();
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

        assert_eq!(
            format!(
                "{:?}",
                CompiledMatcherPlan::compile(&first).unwrap().query()
            ),
            format!(
                "{:?}",
                CompiledMatcherPlan::compile(&second).unwrap().query()
            )
        );
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
        let plan = CompiledMatcherPlan::compile(&decls).unwrap();
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
        let first = CompiledMatcherPlan::compile(&first).unwrap();
        let second = CompiledMatcherPlan::compile(&second).unwrap();
        assert_eq!(first.query().clauses(), second.query().clauses());
        assert_eq!(first.query().clauses(), first.query().clauses());
    }

    #[test]
    fn decl_with_argument_constraint_keeps_call_kind() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .evidence(MatchKind::CallArgument, "fetch")
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile(&[decl]).unwrap();
        let clauses = plan.query().clauses();
        assert_eq!(clauses.len(), 1);
        for left in clauses {
            for right in clauses {
                assert_eq!(left == right, left.cmp(right).is_eq());
            }
        }
    }
}
