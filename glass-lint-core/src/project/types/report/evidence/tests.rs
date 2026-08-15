use super::{EvidenceConstructionError, EvidenceTrace, EvidenceTraces};

#[test]
fn rejects_empty_trace() {
    assert_eq!(
        EvidenceTrace::new(Vec::new()),
        Err(EvidenceConstructionError::EmptyTrace)
    );
}

#[test]
fn rejects_empty_non_truncated_collection() {
    assert_eq!(
        EvidenceTraces::new(Vec::new()),
        Err(EvidenceConstructionError::EmptyTraces)
    );
    assert!(EvidenceTraces::with_truncation(Vec::new(), true).is_ok());
}
