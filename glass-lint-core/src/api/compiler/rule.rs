#[allow(unused_imports)]
pub(crate) use super::{
    CompiledMatcherPlan, EventPredicate, EvidenceDescriptor, IdentityConstraint, IdentityStrength,
    lower_event, lower_identity,
};
use crate::{
    Severity,
    api::{
        classification::RuleIndex,
        rule::{Confidence, MatcherBuildError},
    },
};

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
        compiler::physical,
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
                producer: IdentityConstraint::Rooted { path },
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
