use super::*;

fn index(value: usize) -> RuleIndex {
    RuleIndex::new(value)
}

#[test]
fn flow_limits_defaults_scale_from_flow_operations() {
    let limits = FlowLimits::from_flow_operations(262_144);
    assert!(limits.object_limit() >= 1024);
    assert!(limits.state_limit() >= 4096);
    assert!(limits.emission_limit() >= 1024);
    assert!(limits.mutation_limit() >= 256);
}

#[test]
fn flow_limits_scales_down_to_minimums() {
    let limits = FlowLimits::from_flow_operations(1);
    assert_eq!(limits.object_limit(), 1024);
    assert_eq!(limits.state_limit(), 4096);
    assert_eq!(limits.emission_limit(), 1024);
    assert_eq!(limits.mutation_limit(), 256);
}

#[test]
fn flow_limits_large_operation_budget_does_not_overflow() {
    let limits = FlowLimits::from_flow_operations(usize::MAX);
    assert!(limits.object_limit() >= 1024);
    assert!(limits.state_limit() >= 4096);
    assert!(limits.emission_limit() >= 1024);
    assert!(limits.mutation_limit() >= 256);
    assert!(limits.alternative_limit() >= 16);
}

#[test]
fn flow_limits_accessors_return_configured_values() {
    let limits = FlowLimits::test_new(2048, 8192, 2048, 512);
    assert_eq!(limits.object_limit(), 2048);
    assert_eq!(limits.state_limit(), 8192);
    assert_eq!(limits.emission_limit(), 2048);
    assert_eq!(limits.mutation_limit(), 512);
}

#[test]
fn flow_operation_limit_tracks_the_configured_budget() {
    let limits = FlowLimits::from_flow_operations(1234);
    assert_eq!(limits.operation_limit(), 1234);
}

#[test]
fn flow_id_new_creates_deterministic_identity() {
    let rule = index(5);
    let a = FlowId::new(rule, 3);
    let b = FlowId::new(rule, 3);
    assert_eq!(a, b);
    assert_eq!(a.rule_index(), rule);
    assert_eq!(a.flow_index(), 3);
}

#[test]
fn flow_id_distinguishes_different_rules_and_indices() {
    let a = FlowId::new(index(1), 2);
    let b = FlowId::new(index(1), 3);
    let c = FlowId::new(index(2), 2);
    assert_ne!(a, b);
    assert_ne!(a, c);
}

#[test]
fn indexed_evidence_default_is_empty() {
    let set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
}

#[test]
fn indexed_evidence_insert_and_remove() {
    let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(1));
    set.insert(RequirementIndex::new(1).unwrap(), FactId::from_test(2));
    assert_eq!(set.len(), 2);
    assert!(!set.is_empty());

    set.remove(RequirementIndex::new(0).unwrap());
    assert_eq!(set.len(), 1);
}

#[test]
fn indexed_evidence_values_returns_all_inserted() {
    let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
    set.insert(RequirementIndex::new(2).unwrap(), FactId::from_test(30));
    let values: Vec<_> = set.values().copied().collect();
    assert_eq!(values, vec![FactId::from_test(10), FactId::from_test(30)]);
}

#[test]
fn indexed_evidence_insert_duplicate_key_appends_value() {
    let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
    set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(20));
    let values: Vec<_> = set.values().copied().collect();
    assert_eq!(values.len(), 2);
    assert!(values.contains(&FactId::from_test(10)));
    assert!(values.contains(&FactId::from_test(20)));
    assert_eq!(set.len(), 1);
}

#[test]
fn indexed_evidence_uses_all_64_completion_bits_and_rejects_overflow() {
    let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    assert!(set.insert(RequirementIndex::new(63).unwrap(), FactId::from_test(63)));
    assert!(RequirementIndex::new(64).is_none());
    assert_eq!(set.len(), 1);
    assert_eq!(
        set.values().copied().collect::<Vec<_>>(),
        [FactId::from_test(63)]
    );
}

#[test]
fn requirement_and_sink_indices_preserve_their_domains() {
    let mut requirements: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
    let mut sinks: IndexedEvidence<FactId, SinkIndex> = IndexedEvidence::default();
    assert!(requirements.insert(RequirementIndex::new(63).unwrap(), FactId::from_test(63)));
    assert!(sinks.insert(SinkIndex::new(63).unwrap(), FactId::from_test(63)));
    assert_eq!(
        requirements
            .iter_by_key()
            .find(|(index, _)| *index == RequirementIndex::new(63).unwrap())
            .map(|(_, values)| values.iter().count()),
        Some(1)
    );
    assert_eq!(
        sinks
            .iter_by_key()
            .find(|(index, _)| *index == SinkIndex::new(63).unwrap())
            .map(|(_, values)| values.iter().count()),
        Some(1)
    );
    assert!(RequirementIndex::new(64).is_none());
    assert!(SinkIndex::new(64).is_none());
}

#[test]
fn flow_state_new_creates_unready_state() {
    let flow = FlowId::new(index(0), 0);
    let state = FlowState::new(flow, FactId::from_test(1), FlowObjectId::from_test(0));
    assert_eq!(state.flow_id(), flow);
    assert_eq!(state.source_event(), FactId::from_test(1));
    assert_eq!(state.object_id(), FlowObjectId::from_test(0));
}

#[test]
fn flow_state_key_matches_flow_and_object() {
    let flow = FlowId::new(index(1), 2);
    let state = FlowState::new(flow, FactId::from_test(5), FlowObjectId::from_test(3));
    let key = state.key();
    assert_eq!(key.object(), FlowObjectId::from_test(3));
    assert_eq!(key.flow(), flow);
}

#[test]
fn flow_state_records_and_clears_requirements() {
    let flow = FlowId::new(index(0), 0);
    let mut state = FlowState::new(flow, FactId::from_test(1), FlowObjectId::from_test(0));
    state.record_requirement(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
    state.record_requirement(RequirementIndex::new(1).unwrap(), FactId::from_test(20));
    assert_eq!(state.requirement_entries().count(), 2);

    state.clear_requirement(RequirementIndex::new(0).unwrap());
    assert_eq!(state.requirement_entries().count(), 1);
}
