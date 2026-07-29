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

    pub fn rule_capacity(&self) -> usize {
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
