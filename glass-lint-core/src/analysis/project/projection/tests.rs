use super::*;

#[test]
fn flow_observed_excludes_non_flow_projection_work() {
    let mut outcome = ProjectionOutcome::default();
    let mut local = LocalFlowProjectionOutcome::default();
    local.operations = 7;
    outcome.record_local(&local);
    let cross = flow::cross::CrossProjectionOutcome {
        operations: 5,
        ..flow::cross::CrossProjectionOutcome::default()
    };
    outcome.record_cross(&cross);
    outcome.status.flow.mark_incomplete();
    let finished = outcome.finish();

    assert_eq!(finished.status.flow_observed, Some(12));
}
