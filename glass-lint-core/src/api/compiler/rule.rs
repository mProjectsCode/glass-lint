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
        rule::{ArgumentConstraint, MatcherFamily, MatcherSet, ModuleSpecifierPattern, Rule},
    },
};

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
        let mut clauses = matcher.lower_all();
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

#[derive(Debug, Clone)]
/// Borrowed view of compiled rules selected for a classification run.
pub(crate) struct CompiledRuleSelection<'a> {
    /// All compiled rules, retained for stable rule indexes.
    pub(crate) rules: &'a [CompiledRule],
    /// Sorted selected rule indexes.
    pub(crate) selected: &'a [RuleIndex],
}

impl<'a> CompiledRuleSelection<'a> {
    /// Create a borrowed catalog view over sorted selected indexes.
    pub fn new(rules: &'a [CompiledRule], selected: &'a [RuleIndex]) -> Self {
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
            rule::{
                CallMatcher, FlowCompletion, FlowCondition, Matcher, MatcherSet, MemberCallMatcher,
                ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, ValueMatcher,
            },
        },
    };

    #[test]
    fn every_family_validates_normalizes_flattens_and_compiles() {
        let matcher = MatcherSet::from_matchers(vec![
            Matcher::heuristic_call("fetch"),
            Matcher::rooted_member_call("window.open"),
            Matcher::rooted_member_read("window.location"),
            Matcher::import("node:fs"),
            Matcher::package_import("@scope/pkg").unwrap(),
            Matcher::string_contains("https://"),
            Matcher::heuristic_class("Worker"),
            Matcher::global_constructor("URL"),
            Matcher::from(
                ObjectFlowMatcher::builder("request")
                    .source(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                        "test.method",
                    )))
                    .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                        "ready",
                        ValueMatcher::any_value(),
                    )))
                    .complete_at(FlowCompletion::configuration())
                    .build()
                    .unwrap(),
            ),
            Matcher::returned_member_call("create", "send"),
            Matcher::returned_member_read("create", "token"),
            Matcher::instance_member_call("pkg", "Client", "send"),
        ]);
        matcher
            .validate()
            .expect("all families must survive validation");
        let normalized = matcher.normalized();
        let matchers = normalized.into_matchers();
        assert_eq!(matchers.len(), 12);
        let plan = CompiledMatcherPlan::compile(&MatcherSet::from_matchers(matchers));
        assert!(!plan.query().clauses().is_empty());
        assert_eq!(plan.query().flows().len(), 1);
    }

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
            Matcher::from(
                ObjectFlowMatcher::builder("request")
                    .source(ObjectSourceMatcher::returned_by(MemberCallMatcher::rooted(
                        "test.method",
                    )))
                    .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                        "ready",
                        ValueMatcher::any_value(),
                    )))
                    .complete_at(FlowCompletion::configuration())
                    .build()
                    .unwrap(),
            ),
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
