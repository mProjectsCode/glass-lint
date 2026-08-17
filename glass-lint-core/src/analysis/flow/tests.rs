use crate::analysis::flow::{FlowCompletion, FlowCompletionReason};

#[test]
fn completion_merges_multiple_reasons() {
    let mut completion = FlowCompletion::incomplete(FlowCompletionReason::StateLimit);
    completion.merge(FlowCompletion::incomplete(FlowCompletionReason::TraceArena));

    assert!(completion.is_incomplete());
}

#[test]
fn default_completion_is_complete() {
    assert!(FlowCompletion::default().is_complete());
}
