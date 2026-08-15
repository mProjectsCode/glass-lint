use super::*;

fn state() -> CrossFlowState {
    CrossFlowState::unknown(FlowId::new(RuleIndex::new(0), 0))
}

#[test]
fn context_origin_modes_cannot_match_each_other() {
    let source = CallContext::for_source(
        ModuleId::new(1),
        FunctionId::from_test(2),
        ValueId::from_test(3),
        state(),
        false,
    );
    assert!(source.matches_source_root(ValueId::from_test(3), true, true));
    assert!(!source.matches_parameter(0, true, true));

    let target = CallContext::for_target_call(
        ModuleId::new(1),
        FunctionId::from_test(2),
        0,
        state(),
        false,
    );
    assert!(target.matches_parameter(0, true, true));
    assert!(!target.matches_source_root(ValueId::from_test(3), true, true));

    let unknown = CallContext::for_test(ModuleId::new(1), FunctionId::from_test(2), state());
    assert!(!unknown.matches_parameter(0, true, true));
    assert!(!unknown.matches_source_root(ValueId::from_test(3), true, true));
}
