use std::fmt;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::{
    Severity,
    analysis::matches_global_object_alias,
    api::{
        classification::{MatchKind, RuleIndex},
        compiler::{
            normalize::{self, NormalizedQuery},
            physical::{self, PhysicalPlan},
            validate::validate_query_decl,
        },
        rule::{
            Confidence, MatcherBuildError, ModuleSpecifierPattern,
            query::{EventSpec, IdentitySpec, QueryDecl, limits},
        },
    },
};

/// Canonical compiled matcher plan consumed by analysis.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    physical_plan: PhysicalPlan,
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
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
}

impl IdentityConstraint {
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

    #[allow(dead_code)]
    pub(crate) fn exact_root_matches(&self, source: &SymbolPath) -> bool {
        matches!(self, Self::Rooted { path } if path == source)
    }

    #[allow(dead_code)]
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

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct EvidenceDescriptor {
    pub(crate) kind: MatchKind,
    pub(crate) symbol: String,
}

// ── Lowering: declaration types → compiler IR ────────────────────────────

pub(crate) fn lower_identity(spec: &IdentitySpec) -> IdentityConstraint {
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

pub(crate) fn lower_event(spec: &EventSpec) -> EventPredicate {
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum InvalidQueryClause {
    ImpossibleDimensions,
    ConstraintsRequireCallEvent,
    NonCanonicalConstraints,
    UnavailablePrimaryEvidence,
    InvalidLifecycleRoot,
    ExcessiveArgumentGroups(usize),
    ExcessivePredicateCount(usize),
    ExcessiveAlternatives(usize),
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
            Self::NonCanonicalConstraints => {
                f.write_str("constraints are not in canonical grouped order")
            }
            Self::UnavailablePrimaryEvidence => f.write_str("primary evidence symbol is empty"),
            Self::InvalidLifecycleRoot => f.write_str("lifecycle root is malformed"),
            Self::ExcessiveArgumentGroups(count) => {
                write!(
                    f,
                    "argument group count {count} exceeds limit {}",
                    limits::MAX_ARGUMENT_GROUPS
                )
            }
            Self::ExcessivePredicateCount(count) => {
                write!(
                    f,
                    "predicate count {count} exceeds limit {}",
                    limits::MAX_PREDICATES_PER_ARGUMENT
                )
            }
            Self::ExcessiveAlternatives(count) => {
                write!(
                    f,
                    "static alternative count {count} exceeds limit {}",
                    limits::MAX_STATIC_ALTERNATIVES
                )
            }
        }
    }
}

/// Compile query declarations into a physical plan.
fn compile_queries(queries: &[QueryDecl]) -> Result<PhysicalPlan, MatcherBuildError> {
    let mut all_roots = Vec::new();
    let mut merged_requirements = normalize::PlanRequirements::default();

    for query in queries {
        validate_query_decl(query).map_err(MatcherBuildError::QueryCompileError)?;

        let normalized: NormalizedQuery =
            normalize::normalize_query_decl(query).map_err(MatcherBuildError::QueryCompileError)?;

        let query_plan = physical::plan_normalized(&normalized);
        all_roots.extend(query_plan.roots().iter().cloned());
        merged_requirements.merge_from(query_plan.requirements());
    }

    let mut sorted_roots: Vec<physical::PhysicalRoot> = all_roots;
    sorted_roots.sort();
    sorted_roots.dedup();
    let physical_plan = PhysicalPlan::new(sorted_roots.into_boxed_slice(), merged_requirements);
    physical::validate_physical_plan(&physical_plan)
        .map_err(|e| MatcherBuildError::InvalidLoweredQuery(e.to_string()))?;

    Ok(physical_plan)
}

impl CompiledMatcherPlan {
    pub(crate) fn physical_roots(&self) -> &[physical::PhysicalRoot] {
        self.physical_plan.roots()
    }

    #[allow(dead_code)]
    pub(crate) fn plan_summary(&self) -> String {
        self.physical_plan.summary()
    }

    pub(crate) fn needs_project_overlay(&self) -> bool {
        self.physical_plan.requirements().needs_project_overlay
    }

    pub(crate) fn compile(queries: &[QueryDecl]) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_queries(queries)?;
        Ok(Self { physical_plan })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRuleSelection<'a> {
    pub(crate) rules: &'a [CompiledRuleRecord],
    pub(crate) selected: &'a [RuleIndex],
}

impl<'a> CompiledRuleSelection<'a> {
    pub fn new(rules: &'a [CompiledRuleRecord], selected: &'a [RuleIndex]) -> Self {
        Self { rules, selected }
    }

    pub fn selected_matchers(&self) -> impl Iterator<Item = (RuleIndex, &CompiledMatcherPlan)> {
        self.selected.iter().filter_map(move |&index| {
            self.rules
                .get(index.get())
                .map(|rule| (index, &rule.matcher))
        })
    }

    pub fn is_selected(&self, index: RuleIndex) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    pub fn get(&self, index: RuleIndex) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index.get()).map(|rule| &rule.matcher)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRuleRecord {
    pub(crate) description: String,
    pub(crate) severity: Severity,
    pub(crate) confidence: Confidence,
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRuleRecord {
    pub(crate) fn new(rule: &crate::api::rule::Rule) -> Result<Self, MatcherBuildError> {
        let plan = CompiledMatcherPlan::compile(rule.queries())?;
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
        rule::{EventQuery, QueryDecl, ValueMatcher},
    };

    #[test]
    fn every_declaration_compiles_into_one_plan() {
        let queries = vec![
            QueryDecl::call_global("fetch").unwrap(),
            QueryDecl::member_call_rooted("window.open").unwrap(),
            QueryDecl::member_read_rooted("window.location").unwrap(),
            QueryDecl::import_exact("node:fs").unwrap(),
            QueryDecl::import_package("@scope/pkg").unwrap(),
            QueryDecl::string_contains("https://").unwrap(),
            QueryDecl::class_heuristic("Worker").unwrap(),
            QueryDecl::constructor_global("URL").unwrap(),
            QueryDecl::member_call_returned("create", "send").unwrap(),
            QueryDecl::member_read_returned("create", "token").unwrap(),
            QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
        ];
        let plan = CompiledMatcherPlan::compile(&queries).unwrap();
        assert!(!plan.physical_roots().is_empty());
    }

    #[test]
    fn argument_matcher_compiles_to_constrained_scan() {
        let query = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .into_query()
            .with_evidence(MatchKind::CallArgument, "fetch");
        let plan = CompiledMatcherPlan::compile(&[query]).unwrap();
        let roots = plan.physical_roots();
        assert_eq!(roots.len(), 1);
        match &roots[0] {
            physical::PhysicalRoot::ConstrainedScan {
                constraints,
                evidence,
                ..
            } => {
                assert!(!constraints.groups().is_empty());
                assert_eq!(evidence.kind, MatchKind::CallArgument);
            }
            other => panic!("expected ConstrainedScan, got {other:?}"),
        }
    }

    #[test]
    fn equivalent_declarations_compile_to_identical_queries() {
        let first = vec![
            QueryDecl::call_global("fetch").unwrap(),
            QueryDecl::member_read_rooted("location.href").unwrap(),
        ];
        let second = vec![
            QueryDecl::member_read_rooted("location.href").unwrap(),
            QueryDecl::call_global("fetch").unwrap(),
        ];

        let first = CompiledMatcherPlan::compile(&first).unwrap();
        let second = CompiledMatcherPlan::compile(&second).unwrap();
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }

    #[test]
    fn query_plan_compiles_declarations_into_physical_roots() {
        let roots = {
            let queries = vec![
                QueryDecl::call_global("fetch").unwrap(),
                QueryDecl::member_call_rooted("window.open").unwrap(),
                QueryDecl::member_read_returned("create", "token").unwrap(),
                QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
                QueryDecl::import_exact("node:fs").unwrap(),
                QueryDecl::string_contains("https://").unwrap(),
            ];
            let plan = CompiledMatcherPlan::compile(&queries).unwrap();
            plan.physical_roots().to_vec()
        };
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan {
                identity: IdentityConstraint::Global { name, strength: IdentityStrength::Strict },
                event: EventPredicate::Call, ..
            } if name == "fetch"
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan {
                identity: IdentityConstraint::Rooted { path },
                event: EventPredicate::MemberCall { member }, ..
            } if *path == SymbolPath::from("window.open") && member.eq_chain("window.open")
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::ReturnedSubject {
                identity: IdentityConstraint::Rooted { path },
                event: EventPredicate::MemberRead { member }, ..
            } if path.eq_chain("create") && member.eq_chain("token")
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::InstanceSubject {
                constructor: IdentityConstraint::ModuleExport { module, export },
                member, ..
            } if module == "pkg" && export == "Client" && member.eq_chain("send")
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan {
                event: EventPredicate::Import,
                ..
            }
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan {
                event: EventPredicate::StringReference,
                ..
            }
        )));
    }

    #[test]
    fn query_plan_normalization_is_idempotent_and_order_independent() {
        let first = vec![
            QueryDecl::call_heuristic("fetch").unwrap(),
            QueryDecl::member_read_rooted("location.href").unwrap(),
        ];
        let second = vec![
            QueryDecl::member_read_rooted("location.href").unwrap(),
            QueryDecl::call_heuristic("fetch").unwrap(),
        ];
        let first = CompiledMatcherPlan::compile(&first).unwrap();
        let second = CompiledMatcherPlan::compile(&second).unwrap();
        assert_eq!(
            format!("{:?}", first.physical_roots()),
            format!("{:?}", second.physical_roots())
        );
    }

    #[test]
    fn decl_with_argument_constraint_keeps_call_kind() {
        let query = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap()
            .into_query()
            .with_evidence(MatchKind::CallArgument, "fetch");
        let plan = CompiledMatcherPlan::compile(&[query]).unwrap();
        let roots = plan.physical_roots();
        assert_eq!(roots.len(), 1);
        assert!(matches!(
            &roots[0],
            physical::PhysicalRoot::ConstrainedScan { .. }
        ));
    }
}
