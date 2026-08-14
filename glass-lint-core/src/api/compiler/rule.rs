#[allow(unused_imports)]
pub(crate) use super::{
    CompiledMatcherPlan, EvidenceDescriptor, IdentityConstraint, lower_identity,
};
pub(crate) use crate::api::rule::query::EventSpec;
use crate::{
    RuleId, Severity,
    api::{
        classification::{RuleEvidenceCapacity, RuleIndex},
        rule::{Confidence, MatcherBuildError},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleSelectionError {
    OutOfRange {
        index: RuleIndex,
        capacity: usize,
    },
    Duplicate(RuleIndex),
    Unsorted {
        previous: RuleIndex,
        next: RuleIndex,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRuleSelection<'a> {
    pub(crate) rules: &'a [CompiledRuleRecord],
    pub(crate) selected: &'a [RuleIndex],
}

impl<'a> CompiledRuleSelection<'a> {
    pub fn new(
        rules: &'a [CompiledRuleRecord],
        selected: &'a [RuleIndex],
    ) -> Result<Self, RuleSelectionError> {
        for &index in selected {
            if index.get() >= rules.len() {
                return Err(RuleSelectionError::OutOfRange {
                    index,
                    capacity: rules.len(),
                });
            }
        }
        for pair in selected.windows(2) {
            match pair[0].cmp(&pair[1]) {
                std::cmp::Ordering::Equal => return Err(RuleSelectionError::Duplicate(pair[0])),
                std::cmp::Ordering::Greater => {
                    return Err(RuleSelectionError::Unsorted {
                        previous: pair[0],
                        next: pair[1],
                    });
                }
                std::cmp::Ordering::Less => {}
            }
        }
        Ok(Self { rules, selected })
    }

    pub fn selected_matchers(&self) -> impl Iterator<Item = (RuleIndex, &CompiledMatcherPlan)> {
        self.selected
            .iter()
            .map(move |&index| (index, &self.rules[index.get()].matcher))
    }

    pub fn is_selected(&self, index: RuleIndex) -> bool {
        self.selected.binary_search(&index).is_ok()
    }

    pub fn get(&self, index: RuleIndex) -> Option<&'a CompiledMatcherPlan> {
        self.rules.get(index.get()).map(|rule| &rule.matcher)
    }

    pub(crate) fn evidence_capacity(&self) -> RuleEvidenceCapacity {
        RuleEvidenceCapacity::from_catalog_len(self.rules.len())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRuleRecord {
    pub(crate) rule_id: RuleId,
    pub(crate) description: String,
    pub(crate) query_explanations: Vec<String>,
    pub(crate) severity: Severity,
    pub(crate) confidence: Confidence,
    pub(crate) matcher: CompiledMatcherPlan,
}

impl CompiledRuleRecord {
    pub(crate) fn new(
        rule_id: RuleId,
        rule: &crate::api::rule::Rule,
    ) -> Result<Self, MatcherBuildError> {
        let plan = CompiledMatcherPlan::compile(rule.queries())?;
        Ok(Self {
            rule_id,
            description: rule.description().to_owned(),
            query_explanations: rule
                .queries()
                .iter()
                .map(crate::api::rule::QueryDecl::explanation)
                .collect(),
            severity: rule.severity(),
            confidence: rule.confidence(),
            matcher: plan,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rule::{Confidence, EventQuery, Severity};

    fn record() -> CompiledRuleRecord {
        let matcher =
            CompiledMatcherPlan::compile(&[EventQuery::call_global("fetch").unwrap().into_query()])
                .unwrap();
        CompiledRuleRecord {
            rule_id: RuleId::parse("test:rule").unwrap(),
            description: "test".into(),
            query_explanations: Vec::new(),
            severity: Severity::Warning,
            confidence: Confidence::High,
            matcher,
        }
    }

    #[test]
    fn selection_rejects_out_of_range_indices() {
        let error = CompiledRuleSelection::new(&[], &[RuleIndex::new(0)]).unwrap_err();
        assert_eq!(
            error,
            RuleSelectionError::OutOfRange {
                index: RuleIndex::new(0),
                capacity: 0,
            }
        );
    }

    #[test]
    fn selection_rejects_duplicate_indices() {
        let records = [record()];
        let error = CompiledRuleSelection::new(&records, &[RuleIndex::new(0), RuleIndex::new(0)])
            .unwrap_err();
        assert_eq!(error, RuleSelectionError::Duplicate(RuleIndex::new(0)));
    }

    #[test]
    fn selection_rejects_unsorted_indices() {
        let records = [record(), record()];
        let error = CompiledRuleSelection::new(&records, &[RuleIndex::new(1), RuleIndex::new(0)])
            .unwrap_err();
        assert_eq!(
            error,
            RuleSelectionError::Unsorted {
                previous: RuleIndex::new(1),
                next: RuleIndex::new(0),
            }
        );
    }
}
