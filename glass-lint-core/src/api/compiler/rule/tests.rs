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
    let error =
        CompiledRuleSelection::new(&records, &[RuleIndex::new(0), RuleIndex::new(0)]).unwrap_err();
    assert_eq!(error, RuleSelectionError::Duplicate(RuleIndex::new(0)));
}

#[test]
fn selection_rejects_unsorted_indices() {
    let records = [record(), record()];
    let error =
        CompiledRuleSelection::new(&records, &[RuleIndex::new(1), RuleIndex::new(0)]).unwrap_err();
    assert_eq!(
        error,
        RuleSelectionError::Unsorted {
            previous: RuleIndex::new(1),
            next: RuleIndex::new(0),
        }
    );
}
