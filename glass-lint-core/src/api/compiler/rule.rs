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
            physical::{self, PhysicalPlan},
            validate::{validate_normalized_decl, validate_query_decl},
        },
        rule::{
            Confidence, MatcherBuildError, MatcherDecl, ModuleSpecifierPattern,
            query::{EventSpec, IdentitySpec, QueryDecl, VarId},
        },
    },
};

/// Canonical compiled matcher plan consumed by analysis.  Public matcher
/// declarations are compiled once while a catalog is built and never enter
/// the per-file analysis path.
///
/// This is the sole compiled-plan type.  Consumers access physical roots
/// and flows through accessors; there is no separate plan wrapper and no
/// backward-compat clause storage.
#[derive(Debug, Clone)]
pub(crate) struct CompiledMatcherPlan {
    /// Physical plan (Phase 6): executable operators produced by the planner.
    physical_plan: PhysicalPlan,
    flows: Box<[CompiledObjectFlow]>,
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

/// Compile declarations into a physical plan.
///
/// Each declaration is converted to a logical query, validated, normalized,
/// and planned into physical roots.  Roots are sorted for deterministic
/// execution order across equivalent queries.
fn compile_declarations(decls: &[MatcherDecl]) -> Result<PhysicalPlan, MatcherBuildError> {
    let mut all_roots = Vec::new();
    let mut merged_requirements = normalize::PlanRequirements::default();

    for (i, decl) in decls.iter().enumerate() {
        let var_id = VarId::new(u32::try_from(i).unwrap_or(u32::MAX));

        // Phase 4: Validate each declaration as a logical QueryDecl.
        let query = QueryDecl::from_matcher(decl, var_id);
        validate_query_decl(&query).map_err(|e| {
            MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
        })?;

        // Phase 5: Normalize the logical query into canonical form.
        let (normalized, requirements) = normalize::normalize_query_decl(&query);
        validate_normalized_decl(&normalized).map_err(|e| {
            MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
        })?;

        // Phase 6: Plan the normalized query into physical roots.
        let query_plan = physical::plan_normalized(&normalized, requirements);
        all_roots.extend(query_plan.roots().iter().cloned());
        merged_requirements.merge_from(query_plan.requirements());
    }

    // Sort and deduplicate roots for deterministic order across equivalent
    // queries.  Deduplication ensures that identical declarations produce
    // one root rather than duplicated work.
    let mut sorted_roots: Vec<physical::PhysicalRoot> = all_roots;
    sorted_roots.sort();
    sorted_roots.dedup();
    let physical_plan = PhysicalPlan::new(sorted_roots.into_boxed_slice(), merged_requirements);
    physical::validate_physical_plan(&physical_plan, 0)
        .map_err(|e| MatcherBuildError::InvalidLoweredQuery(e.to_string()))?;

    Ok(physical_plan)
}

/// Compile a single flow matcher into a physical plan, routed through the
/// validate→normalize→plan pipeline.
fn compile_single_flow(
    flow: &crate::api::rule::ObjectFlowMatcher,
) -> Result<PhysicalPlan, MatcherBuildError> {
    flow.validate()?;

    let query = QueryDecl::from_flow_matcher(flow, VarId::new(0));
    validate_query_decl(&query).map_err(|e| {
        MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
    })?;

    let (normalized, requirements) = normalize::normalize_query_decl(&query);
    validate_normalized_decl(&normalized).map_err(|e| {
        MatcherBuildError::InvalidLoweredQuery(format!("{}: {}", e.diagnostic_name(), e))
    })?;

    Ok(physical::plan_normalized(&normalized, requirements))
}

impl CompiledMatcherPlan {
    pub(crate) fn flows(&self) -> &[CompiledObjectFlow] {
        &self.flows
    }

    pub(crate) fn physical_roots(&self) -> &[physical::PhysicalRoot] {
        self.physical_plan.roots()
    }

    #[allow(dead_code)]
    pub(crate) fn plan_summary(&self) -> String {
        self.physical_plan.summary()
    }

    /// Compile declarations into a physical plan.  Used by test helpers.
    #[cfg(test)]
    pub(crate) fn compile_decls(decls: &[MatcherDecl]) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_declarations(decls)?;
        Ok(Self {
            physical_plan,
            flows: Box::new([]),
        })
    }

    /// Compile declarations and object flows into a complete plan.
    ///
    /// Flow matchers are lowered to [`QueryDecl`] lifecycle queries,
    /// validated, normalized, and planned through the same pipeline as
    /// ordinary declarations.  The resulting lifecycle roots embed
    /// [`CompiledObjectFlow`] values directly.
    pub(crate) fn compile_decls_and_flows(
        decls: &[MatcherDecl],
        flows: &[crate::api::rule::ObjectFlowMatcher],
    ) -> Result<Self, MatcherBuildError> {
        let physical_plan = compile_declarations(decls)?;
        let mut all_roots: Vec<physical::PhysicalRoot> = physical_plan.roots().to_vec();
        let mut merged_requirements = physical_plan.requirements().clone();

        for flow in flows {
            let flow_plan = compile_single_flow(flow)?;
            all_roots.extend(flow_plan.roots().iter().cloned());
            merged_requirements.merge_from(flow_plan.requirements());
        }

        // Sort and deduplicate for deterministic order.
        all_roots.sort();
        all_roots.dedup();
        let physical_plan = PhysicalPlan::new(all_roots.into_boxed_slice(), merged_requirements);
        physical::validate_physical_plan(&physical_plan, 0)
            .map_err(|e| MatcherBuildError::InvalidLoweredQuery(e.to_string()))?;

        // Extract compiled flows from lifecycle roots for analysis
        // consumer compatibility.
        let compiled_flows: Vec<CompiledObjectFlow> = physical_plan
            .roots()
            .iter()
            .filter_map(|root| {
                if let physical::PhysicalRoot::Lifecycle { flow } = root {
                    Some(flow.clone())
                } else {
                    None
                }
            })
            .collect();

        Ok(Self {
            physical_plan,
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
        assert!(!plan.physical_roots().is_empty());
    }

    #[test]
    fn argument_matcher_compiles_to_constrained_scan() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .evidence(MatchKind::CallArgument, "fetch")
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile_decls(&[decl]).unwrap();
        let roots = plan.physical_roots();
        assert_eq!(roots.len(), 1);
        match &roots[0] {
            physical::PhysicalRoot::ConstrainedScan {
                constraints,
                evidence,
                ..
            } => {
                assert!(!constraints.is_empty());
                assert_eq!(evidence.kind, MatchKind::CallArgument);
            }
            other => panic!("expected ConstrainedScan, got {other:?}"),
        }
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
    fn query_plan_compiles_declarations_into_physical_roots() {
        let roots = {
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
            plan.physical_roots().to_vec()
        };
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan { identity: IdentityConstraint::Global { name, strength: IdentityStrength::Strict }, event: EventPredicate::Call, .. } if name == "fetch"
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::IndexedScan { identity: IdentityConstraint::Rooted { path }, event: EventPredicate::MemberCall { member }, .. } if *path == SymbolPath::from("window.open") && member.eq_chain("window.open")
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::ReturnedSubject { identity: IdentityConstraint::Rooted { path }, event: EventPredicate::MemberRead { member }, .. } if path.eq_chain("create") && member.eq_chain("token")
        )));
        assert!(roots.iter().any(|root| matches!(
            root,
            physical::PhysicalRoot::InstanceSubject { constructor: IdentityConstraint::ModuleExport { module, export }, member, .. } if module == "pkg" && export == "Client" && member.eq_chain("send")
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
        assert_eq!(
            format!("{:?}", first.physical_roots()),
            format!("{:?}", second.physical_roots())
        );
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
        let roots = plan.physical_roots();
        assert_eq!(roots.len(), 1);
        assert!(matches!(
            &roots[0],
            physical::PhysicalRoot::ConstrainedScan { .. }
        ));
    }
}
