use super::*;

#[test]
fn accounting_owns_bounded_counters() {
    let mut metrics = ProjectLoadMetrics::default();
    metrics.record_files(3);
    metrics.record_edge();
    metrics.admit_requests(2, 2).unwrap();
    metrics.admit_source_bytes(11, 11).unwrap();

    assert_eq!(metrics.files(), 3);
    assert_eq!(metrics.edges(), 1);
    assert_eq!(metrics.requests(), 2);
    assert_eq!(metrics.bytes(), 11);
}

#[test]
fn accounting_rejects_over_budget_updates() {
    let mut metrics = ProjectLoadMetrics::default();
    assert!(metrics.admit_requests(2, 1).is_err());
    assert!(metrics.admit_source_bytes(2, 1).is_err());
}
