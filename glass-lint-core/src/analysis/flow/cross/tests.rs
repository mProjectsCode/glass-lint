use super::*;
use crate::{
    analysis::{
        facts::FactId,
        flow::cross::{
            sources::{SourceCandidate, SourceKey},
            state::{CallContext, CrossFlowState, SourceBudget},
        },
        model::{flow::FlowId, scope::FunctionId, value::ValueId},
        trace::QualifiedEvent,
    },
    api::classification::RuleIndex,
    project::ModuleId,
};

fn key(module: u32, function: u32, value: u32) -> SourceKey {
    SourceKey::new(
        ModuleId::new(module),
        FunctionId::from_test(function),
        ValueId::from_test(value),
    )
}

fn candidate(rule: usize, flow: usize, fact: u32) -> SourceCandidate {
    SourceCandidate::new(
        FlowId::new(RuleIndex::new(rule), flow),
        FactId::from_test(fact),
    )
}

#[test]
fn propagate_transfers_along_adjacency_edge() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let from = key(1, 1, 1);
    let to = key(1, 1, 2);

    sources.add_candidate(from, candidate(0, 0, 10));
    sources.add_candidate(from, candidate(0, 0, 20));
    sources.add_edge(from, to);

    assert!(sources.propagate(&mut budget).is_complete());

    assert_eq!(sources.candidate_count(&to), 2);
    assert!(sources.contains_candidate(&to, candidate(0, 0, 10)));
    assert!(sources.contains_candidate(&to, candidate(0, 0, 20)));
}

#[test]
fn propagate_deduplicates_by_construction() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let from = key(1, 1, 1);
    let to = key(1, 1, 2);

    sources.add_candidate(from, candidate(0, 0, 10));
    sources.add_edge(from, to);

    assert!(sources.propagate(&mut budget).is_complete());
    assert_eq!(sources.candidate_count(&to), 1);

    // Second propagation is a no-op because candidates are already at the
    // destination.
    assert!(sources.propagate(&mut budget).is_complete());
    assert_eq!(sources.candidate_count(&to), 1);
}

#[test]
fn propagate_partial_novelty() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let from = key(1, 1, 1);
    let to = key(1, 1, 2);

    sources.add_candidate(from, candidate(0, 0, 10));
    sources.add_candidate(from, candidate(0, 0, 20));
    sources.add_candidate(to, candidate(0, 0, 10));
    sources.add_edge(from, to);

    assert!(sources.propagate(&mut budget).is_complete());
    assert_eq!(sources.candidate_count(&to), 2);

    assert!(sources.propagate(&mut budget).is_complete());
}

#[test]
fn propagate_missing_source_is_no_op() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let from = key(1, 1, 1);
    let to = key(1, 1, 2);

    sources.add_edge(from, to);

    assert!(sources.propagate(&mut budget).is_complete());
    assert!(!sources.has_candidates(&to));
    assert!(!sources.has_candidates(&from));
}

#[test]
fn propagate_self_edge_is_skipped() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let k = key(1, 1, 1);
    sources.add_candidate(k, candidate(0, 0, 10));
    sources.add_edge(k, k);

    assert!(sources.propagate(&mut budget).is_complete());
    assert_eq!(sources.candidate_count(&k), 1);
}

#[test]
fn propagate_multi_hop() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let a = key(1, 1, 1);
    let b = key(1, 1, 2);
    let c = key(1, 1, 3);

    sources.add_candidate(a, candidate(0, 0, 10));
    sources.add_edge(a, b);
    sources.add_edge(b, c);

    assert!(sources.propagate(&mut budget).is_complete());

    assert_eq!(sources.candidate_count(&b), 1);
    assert!(sources.contains_candidate(&b, candidate(0, 0, 10)));
    assert_eq!(sources.candidate_count(&c), 1);
    assert!(sources.contains_candidate(&c, candidate(0, 0, 10)));
}

#[test]
fn propagate_multi_hop_converges() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let a = key(1, 1, 1);
    let b = key(1, 1, 2);

    sources.add_candidate(a, candidate(0, 0, 10));
    sources.add_edge(a, b);
    sources.add_edge(b, a);

    let completion = sources.propagate(&mut budget);
    assert!(completion.is_complete());
    assert!(sources.contains_candidate(&b, candidate(0, 0, 10)));
}

#[test]
fn propagate_preserves_ordering_at_destination() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let from = key(1, 1, 1);
    let to = key(1, 1, 2);

    sources.add_candidate(to, candidate(0, 0, 5));
    sources.add_candidate(from, candidate(0, 1, 20));
    sources.add_candidate(from, candidate(0, 0, 10));
    sources.add_edge(from, to);

    sources.propagate(&mut budget);

    let ordered: Vec<_> = sources.candidates(&to).copied().collect();
    assert_eq!(ordered[0], candidate(0, 0, 5));
    assert_eq!(ordered[1], candidate(0, 0, 10));
    assert_eq!(ordered[2], candidate(0, 1, 20));
}

#[test]
fn propagate_pending_limit_exhausted() {
    let mut sources = FlowSources::default();
    let mut budget = Budget::new(usize::MAX);
    let a = key(1, 1, 1);
    let b = key(1, 1, 2);
    for i in 0..(u32::try_from(MAX_PENDING).unwrap_or(u32::MAX) + 10) {
        sources.add_candidate(a, candidate(0, 0, i));
    }
    // a → b edges cause all candidates to flow into b in one round,
    // filling the pending queue past the safety limit.
    sources.add_edge(a, b);

    assert!(sources.propagate(&mut budget).is_incomplete());
}

#[test]
fn source_budget_transfer_limit_is_detected() {
    let mut budget = SourceBudget::new(10);
    for _ in 0..10 {
        assert!(budget.try_charge());
        assert!(!budget.exhausted());
    }
    assert!(!budget.try_charge());
    assert!(budget.exhausted());
}

#[test]
fn source_budget_not_exhausted_after_stabilization() {
    let mut budget = SourceBudget::new(100);
    assert!(budget.try_charge());
    assert!(!budget.exhausted());
}

#[test]
fn source_candidate_ordering_is_deterministic() {
    let mut sources = FlowSources::default();
    let k = key(1, 1, 1);

    sources.add_candidate(k, candidate(0, 2, 30));
    sources.add_candidate(k, candidate(0, 0, 10));
    sources.add_candidate(k, candidate(0, 1, 20));

    let ordered: Vec<_> = sources.candidates(&k).copied().collect();
    assert_eq!(ordered[0], candidate(0, 0, 10));
    assert_eq!(ordered[1], candidate(0, 1, 20));
    assert_eq!(ordered[2], candidate(0, 2, 30));
}

fn context(module: u32, function: u32) -> CallContext {
    CallContext::for_test(
        ModuleId::new(module),
        FunctionId::from_test(function),
        CrossFlowState::known(
            FlowId::new(RuleIndex::new(0), 0),
            QualifiedEvent::new(ModuleId::new(1), FactId::from_test(1)),
        ),
    )
}

#[test]
fn worklist_len_counts_total_retained_not_pending() {
    let mut wl = ContextWorklist::new(10);
    assert_eq!(wl.len(), 0);

    // Push two contexts
    assert_eq!(wl.push(context(1, 1)), worklist::FifoAdmission::Inserted);
    assert_eq!(wl.len(), 1);
    assert_eq!(wl.push(context(1, 2)), worklist::FifoAdmission::Inserted);
    assert_eq!(wl.len(), 2);

    // Pop one — seen still retains both, so len is still 2
    let _popped = wl.pop_front();
    assert_eq!(wl.len(), 2);

    // Duplicate push does not increase retained count
    assert_eq!(wl.push(context(1, 1)), worklist::FifoAdmission::Duplicate);
    assert_eq!(wl.len(), 2);
}

#[test]
fn worklist_respects_max_retained_limit() {
    let mut wl = ContextWorklist::new(3);
    assert_eq!(wl.push(context(1, 1)), worklist::FifoAdmission::Inserted);
    assert_eq!(wl.push(context(1, 2)), worklist::FifoAdmission::Inserted);
    assert_eq!(wl.push(context(1, 3)), worklist::FifoAdmission::Inserted);
    assert!(!wl.is_exhausted());
    // Fourth unique context hits the limit
    assert_eq!(wl.push(context(1, 4)), worklist::FifoAdmission::Full);
    assert_eq!(wl.len(), 3);
    assert!(wl.is_exhausted());
}

#[test]
fn worklist_is_exhausted_false_below_limit() {
    let mut wl = ContextWorklist::new(5);
    assert!(!wl.is_exhausted());
    assert_eq!(wl.push(context(1, 1)), worklist::FifoAdmission::Inserted);
    assert!(!wl.is_exhausted());
}
