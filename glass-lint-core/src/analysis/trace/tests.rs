use super::*;

#[test]
fn interning_shares_identical_nodes() {
    let mut arena = TraceArena::new(100);
    let id1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    let id2 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    assert_eq!(id1, id2);
    assert_eq!(arena.node_count(), 1);
}

#[test]
fn different_events_get_different_ids() {
    let mut arena = TraceArena::new(100);
    let id1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    let id2 = arena.intern(None, qe(0, 2), EvidenceRole::Source).unwrap();
    assert_ne!(id1, id2);
    assert_eq!(arena.node_count(), 2);
}

#[test]
fn parent_distinguishes_nodes() {
    let mut arena = TraceArena::new(100);
    let parent = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    let child1 = arena
        .intern(Some(parent), qe(0, 2), EvidenceRole::Sink)
        .unwrap();
    let child2 = arena.intern(None, qe(0, 2), EvidenceRole::Sink).unwrap();
    assert_ne!(child1, child2);
    assert_eq!(arena.node_count(), 3);
}

#[test]
fn reconstruct_trace_walks_parent_chain() {
    let mut arena = TraceArena::new(100);
    let step1 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    let step2 = arena
        .intern(Some(step1), qe(0, 2), EvidenceRole::Requirement)
        .unwrap();
    let step3 = arena
        .intern(Some(step2), qe(0, 3), EvidenceRole::Sink)
        .unwrap();
    let trace = arena.reconstruct_trace(step3).unwrap();
    assert_eq!(trace.len(), 3);
    assert_eq!(trace[0], TraceStep::new(qe(0, 1), EvidenceRole::Source));
    assert_eq!(
        trace[1],
        TraceStep::new(qe(0, 2), EvidenceRole::Requirement)
    );
    assert_eq!(trace[2], TraceStep::new(qe(0, 3), EvidenceRole::Sink));
}

#[test]
fn reconstruct_single_step_trace() {
    let mut arena = TraceArena::new(100);
    let head = arena
        .intern(None, qe(1, 5), EvidenceRole::Occurrence)
        .unwrap();
    let trace = arena.reconstruct_trace(head).unwrap();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0], TraceStep::new(qe(1, 5), EvidenceRole::Occurrence));
}

#[test]
fn arena_exhaustion_returns_none() {
    let mut arena = TraceArena::new(2);
    assert!(arena.intern(None, qe(0, 1), EvidenceRole::Source).is_some());
    assert!(
        arena
            .intern(None, qe(0, 2), EvidenceRole::Requirement)
            .is_some()
    );
    assert!(arena.intern(None, qe(0, 3), EvidenceRole::Sink).is_none());
    assert!(arena.is_exhausted());
}

#[test]
fn deterministic_ordering_of_nodes() {
    let mut arena = TraceArena::new(100);
    let id1 = arena.intern(None, qe(0, 2), EvidenceRole::Source).unwrap();
    let id2 = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    assert_eq!(arena.node(id1).unwrap().event, qe(0, 2));
    assert_eq!(arena.node(id2).unwrap().event, qe(0, 1));
}

#[test]
fn cross_module_qualification() {
    let mut arena = TraceArena::new(100);
    let ev1 = arena.intern(None, qe(1, 10), EvidenceRole::Source).unwrap();
    let ev2 = arena
        .intern(Some(ev1), qe(2, 20), EvidenceRole::Sink)
        .unwrap();
    let trace = arena.reconstruct_trace(ev2).unwrap();
    assert_eq!(trace[0].event().module, ModuleId::new(1));
    assert_eq!(trace[0].event().fact.raw_for_test(), 10);
    assert_eq!(trace[1].event().module, ModuleId::new(2));
    assert_eq!(trace[1].event().fact.raw_for_test(), 20);
}

#[test]
fn exhausted_arena_does_not_affect_earlier_nodes() {
    let mut arena = TraceArena::new(1);
    let id = arena.intern(None, qe(0, 1), EvidenceRole::Source).unwrap();
    assert!(arena.intern(None, qe(0, 2), EvidenceRole::Sink).is_none());
    let trace = arena.reconstruct_trace(id).unwrap();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0], TraceStep::new(qe(0, 1), EvidenceRole::Source));
}

#[test]
fn foreign_handles_are_rejected_and_reconstruction_is_explicitly_invalid() {
    let mut arena = TraceArena::new(10);
    let mut foreign = TraceArena::new(10);
    let foreign_head = foreign
        .intern(None, qe(1, 1), EvidenceRole::Source)
        .unwrap();

    assert!(
        arena
            .intern(Some(foreign_head), qe(0, 1), EvidenceRole::Sink)
            .is_none()
    );
    assert!(arena.is_exhausted());
    assert_eq!(arena.reconstruct_trace(foreign_head), None);
}
