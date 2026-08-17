use super::*;
use crate::{
    analysis::model::{
        fact::{ControlRegionId, FactId},
        flow::{FlowId, FlowState, RequirementIndex, SinkIndex},
        value::{FlowObjectId, ValueId},
    },
    api::classification::RuleIndex,
};

#[test]
fn mismatched_region_pop_preserves_the_top_frame() {
    let mut stack = ControlStack::default();
    stack.push(ControlFrame::Branch {
        region: ControlRegionId::from_test(1),
        base: vec![FlowEnvironment::initial()],
        then_exit: None,
    });

    assert!(matches!(
        stack.pop_region(ControlRegionId::from_test(2)),
        Err(ControlStackError::WrongRegion)
    ));
    assert!(matches!(
        stack.last_mut(),
        Some(ControlFrame::Branch { .. })
    ));
    assert!(stack.pop_region(ControlRegionId::from_test(1)).is_ok());
}

#[test]
fn wrong_function_exit_preserves_the_top_frame() {
    let mut stack = ControlStack::default();
    stack.push(ControlFrame::Branch {
        region: ControlRegionId::from_test(1),
        base: vec![FlowEnvironment::initial()],
        then_exit: None,
    });

    assert_eq!(stack.pop_function(), Err(ControlStackError::WrongKind));
    assert!(matches!(
        stack.last_mut(),
        Some(ControlFrame::Branch { .. })
    ));
}

#[test]
fn empty_loop_operations_report_missing_frames() {
    let mut stack = ControlStack::default();
    assert_eq!(stack.take_loop_continues(), Err(ControlStackError::Empty));
    assert_eq!(
        stack.new_loop_breaks_since(0),
        Err(ControlStackError::Empty)
    );
}

fn test_evidence() -> ClassificationEvidence {
    ClassificationEvidence::from_occurrences(
        crate::api::rule::MatchKind::CallArgument,
        "test".to_owned(),
        vec![
            crate::api::classification::ClassificationEvidenceOccurrence::new(
                glass_lint_datastructures::ByteRange::empty(),
                Some(1),
                None,
            ),
        ],
        crate::project::MatchCertainty::Definite,
    )
    .expect("test evidence has one occurrence")
}

#[test]
fn checkpoints_restore_divergent_mutation_paths() {
    let mut table = FlowStateTable::new(262_144, 4096);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(1));
    let base = table.capture(true);

    table.bind(ValueId::from_test(2), FlowObjectId::from_test(2));
    let left = table.capture(true);
    assert!(table.restore(base));
    assert_eq!(table.object_for(ValueId::from_test(2)), None);

    table.bind(ValueId::from_test(3), FlowObjectId::from_test(3));
    assert!(table.restore(left));
    assert_eq!(
        table.object_for(ValueId::from_test(2)),
        Some(FlowObjectId::from_test(2))
    );
    assert_eq!(table.object_for(ValueId::from_test(3)), None);
    assert!(table.restore(base));
    assert_eq!(
        table.object_for(ValueId::from_test(1)),
        Some(FlowObjectId::from_test(1))
    );
}

#[test]
fn redo_requirement_removal_does_not_restore_removed_events() {
    let mut table = FlowStateTable::new(100, 100);
    let flow = FlowId::new(RuleIndex::new(0), 0);
    table.admit_object(
        &[],
        FlowObjectId::from_test(10),
        vec![FlowState::new(
            flow,
            FactId::from_test(1),
            FlowObjectId::from_test(10),
        )],
    );

    assert!(table.record_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
        FactId::from_test(5),
    ));
    let configured = table.capture(true);

    assert!(table.clear_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
    ));
    assert!(table.record_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
        FactId::from_test(7),
    ));
    let updated = table.capture(true);

    assert!(table.restore(configured));
    assert!(table.restore(updated));

    let state = table.state(FlowObjectId::from_test(10), flow).unwrap();
    assert_eq!(
        state.requirement_entries().next().map(|(_, events)| events),
        Some(vec![FactId::from_test(7)])
    );
}

#[test]
fn bind_updates_and_unbind_removes_aliases() {
    let mut table = FlowStateTable::new(100, 100);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(10));
    assert_eq!(
        table.object_for(ValueId::from_test(1)),
        Some(FlowObjectId::from_test(10))
    );
    assert!(table.has_alias_for(FlowObjectId::from_test(10)));

    table.bind(ValueId::from_test(1), FlowObjectId::from_test(20));
    assert_eq!(
        table.object_for(ValueId::from_test(1)),
        Some(FlowObjectId::from_test(20))
    );

    let removed = table.unbind(ValueId::from_test(1));
    assert_eq!(removed, Some(FlowObjectId::from_test(20)));
    assert_eq!(table.object_for(ValueId::from_test(1)), None);
    assert!(!table.has_alias_for(FlowObjectId::from_test(20)));
}

#[test]
fn object_for_returns_none_for_unbound_value() {
    let table = FlowStateTable::new(100, 100);
    assert_eq!(table.object_for(ValueId::from_test(99)), None);
}

#[test]
fn has_alias_for_false_when_no_aliases_exist() {
    let table = FlowStateTable::new(100, 100);
    assert!(!table.has_alias_for(FlowObjectId::from_test(1)));
}

#[test]
fn objects_are_unique_for_multiple_aliases() {
    let mut table = FlowStateTable::new(100, 100);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(1));
    table.bind(ValueId::from_test(2), FlowObjectId::from_test(1));
    table.bind(ValueId::from_test(3), FlowObjectId::from_test(2));

    assert_eq!(
        table.objects().collect::<Vec<_>>(),
        vec![FlowObjectId::from_test(1), FlowObjectId::from_test(2)]
    );
}

#[test]
fn unbind_aliases_cleans_state_only_after_the_last_alias() {
    let mut table = FlowStateTable::new(100, 100);
    let aliases = [ValueId::from_test(1), ValueId::from_test(2)];
    let object = FlowObjectId::from_test(1);
    table.bind_aliases(&aliases, object);
    table.admit_object(
        &[],
        object,
        vec![FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            object,
        )],
    );

    table.unbind_aliases(&aliases[..1]);
    assert_eq!(table.states_for(object).count(), 1);
    table.unbind_aliases(&aliases[1..]);
    assert_eq!(table.states_for(object).count(), 0);
}

#[test]
fn state_limit_rejects_insertion_beyond_capacity() {
    let mut table = FlowStateTable::new(2, 100);
    let state1 = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(1),
        FlowObjectId::from_test(1),
    );
    let state2 = FlowState::new(
        FlowId::new(RuleIndex::new(0), 1),
        FactId::from_test(2),
        FlowObjectId::from_test(2),
    );
    let state3 = FlowState::new(
        FlowId::new(RuleIndex::new(0), 2),
        FactId::from_test(3),
        FlowObjectId::from_test(3),
    );
    table.admit_object(&[], FlowObjectId::from_test(1), vec![state1]);
    table.admit_object(&[], FlowObjectId::from_test(2), vec![state2]);
    table.admit_object(&[], FlowObjectId::from_test(3), vec![state3]);
    assert_eq!(table.state_count(), 2);
    assert!(table.state_limit_rejected());
}

#[test]
fn admit_object_counts_updates_without_rejecting_the_batch() {
    let mut table = FlowStateTable::new(2, 100);
    let existing = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(1),
        FlowObjectId::from_test(1),
    );
    table.admit_object(&[], FlowObjectId::from_test(1), vec![existing]);
    let update = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(2),
        FlowObjectId::from_test(1),
    );
    let new_state = FlowState::new(
        FlowId::new(RuleIndex::new(0), 1),
        FactId::from_test(3),
        FlowObjectId::from_test(2),
    );

    table.admit_object(
        &[ValueId::from_test(2)],
        FlowObjectId::from_test(2),
        vec![update, new_state],
    );
    assert_eq!(
        table.object_for(ValueId::from_test(2)),
        Some(FlowObjectId::from_test(2))
    );
    assert_eq!(table.state_count(), 2);
    assert_eq!(
        table
            .state(
                FlowObjectId::from_test(1),
                FlowId::new(RuleIndex::new(0), 0)
            )
            .map(FlowState::source_event),
        Some(FactId::from_test(2))
    );
}

#[test]
fn rejected_object_admission_does_not_bind_or_insert() {
    let mut table = FlowStateTable::new(1, 100);
    let existing = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(1),
        FlowObjectId::from_test(1),
    );
    table.admit_object(&[], FlowObjectId::from_test(1), vec![existing]);
    let rejected = FlowState::new(
        FlowId::new(RuleIndex::new(0), 1),
        FactId::from_test(2),
        FlowObjectId::from_test(2),
    );

    table.admit_object(
        &[ValueId::from_test(2)],
        FlowObjectId::from_test(2),
        vec![rejected],
    );
    assert_eq!(table.object_for(ValueId::from_test(2)), None);
    assert_eq!(table.state_count(), 1);
    assert!(table.state_limit_rejected());
}

#[test]
fn remove_states_for_clears_all_object_states() {
    let mut table = FlowStateTable::new(100, 100);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(1));
    table.bind(ValueId::from_test(2), FlowObjectId::from_test(1));
    let s1 = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(1),
        FlowObjectId::from_test(1),
    );
    let s2 = FlowState::new(
        FlowId::new(RuleIndex::new(0), 1),
        FactId::from_test(2),
        FlowObjectId::from_test(2),
    );
    table.admit_object(&[], FlowObjectId::from_test(1), vec![s1]);
    table.admit_object(&[], FlowObjectId::from_test(2), vec![s2]);
    table.remove_states_for(FlowObjectId::from_test(1));
    assert_eq!(table.states_for(FlowObjectId::from_test(1)).count(), 0);
    assert_eq!(table.state_count(), 1);
}

#[test]
fn mutation_count_tracks_mutations() {
    let mut table = FlowStateTable::new(100, 100);
    assert_eq!(table.mutation_count(), 0);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(10));
    assert_eq!(table.mutation_count(), 1);
    table.bind(ValueId::from_test(2), FlowObjectId::from_test(20));
    assert_eq!(table.mutation_count(), 2);
    table.unbind(ValueId::from_test(1));
    assert_eq!(table.mutation_count(), 3);
}

#[test]
fn clear_removes_all_aliases_and_states() {
    let mut table = FlowStateTable::new(100, 100);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(10));
    table.bind(ValueId::from_test(2), FlowObjectId::from_test(20));
    let s = FlowState::new(
        FlowId::new(RuleIndex::new(0), 0),
        FactId::from_test(1),
        FlowObjectId::from_test(10),
    );
    table.admit_object(&[], FlowObjectId::from_test(10), vec![s]);
    table.clear();
    assert_eq!(table.object_for(ValueId::from_test(1)), None);
    assert_eq!(table.object_for(ValueId::from_test(2)), None);
    assert_eq!(table.state_count(), 0);
}

#[test]
fn distinct_semantic_snapshots_remain_distinct() {
    let mut table = FlowStateTable::new(100, 100);
    table.bind(ValueId::from_test(1), FlowObjectId::from_test(1));
    let first = table.semantic_snapshot();

    table.bind(ValueId::from_test(2), FlowObjectId::from_test(2));
    let second = table.semantic_snapshot();

    assert_ne!(first, second);
}

#[test]
fn evidence_limit_rejects_repeated_emissions_for_existing_key() {
    let mut items = RuleEvidenceTable::new_for_test(1);
    let mut evidence = FlowEvidence::new(&mut items, 1);
    let key = ReportEvidenceKey::new(
        RuleIndex::new(0),
        0,
        FlowObjectId::from_test(1),
        FactId::from_test(1),
    );

    assert!(evidence.record_if_admitted(key, RuleIndex::new(0), test_evidence(),));
    assert!(!evidence.record_if_admitted(key, RuleIndex::new(0), test_evidence(),));
    assert_eq!(evidence.emitted_count(), 1);
    assert!(evidence.limit_rejected());
}

#[test]
fn evidence_limit_rejects_new_keys_after_capacity_is_full() {
    let mut items = RuleEvidenceTable::new_for_test(1);
    let mut evidence = FlowEvidence::new(&mut items, 2);
    let first = ReportEvidenceKey::new(
        RuleIndex::new(0),
        0,
        FlowObjectId::from_test(1),
        FactId::from_test(1),
    );
    let second = ReportEvidenceKey::new(
        RuleIndex::new(0),
        0,
        FlowObjectId::from_test(2),
        FactId::from_test(2),
    );

    assert!(evidence.record_if_admitted(first, RuleIndex::new(0), test_evidence(),));
    assert!(evidence.record_if_admitted(second, RuleIndex::new(0), test_evidence(),));
    assert!(!evidence.record_if_admitted(first, RuleIndex::new(0), test_evidence(),));
    assert!(!evidence.record_if_admitted(second, RuleIndex::new(0), test_evidence(),));
    assert_eq!(evidence.emitted_count(), 2);
    assert!(evidence.limit_rejected());
}

#[test]
fn fine_grained_state_edits_restore_across_checkpoints() {
    let mut table = FlowStateTable::new(100, 100);
    let flow = FlowId::new(RuleIndex::new(0), 0);
    let state = FlowState::new(flow, FactId::from_test(1), FlowObjectId::from_test(10));
    table.admit_object(&[], FlowObjectId::from_test(10), vec![state]);
    let base = table.capture(true);
    assert!(table.record_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
        FactId::from_test(5),
    ));
    assert!(table.record_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
        FactId::from_test(7),
    ));
    assert!(table.record_sink(
        FlowObjectId::from_test(10),
        flow,
        SinkIndex::new(0).unwrap(),
        FactId::from_test(6),
    ));
    let retrieved = table.state(FlowObjectId::from_test(10), flow).unwrap();
    assert_eq!(retrieved.source_event(), FactId::from_test(1));
    assert_eq!(retrieved.requirement_entries().count(), 1);
    assert_eq!(retrieved.sink_entries().count(), 1);

    let configured = table.capture(true);
    assert!(table.clear_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(0).unwrap(),
    ));
    assert_eq!(
        table
            .state(FlowObjectId::from_test(10), flow)
            .unwrap()
            .requirement_entries()
            .count(),
        0
    );
    assert!(table.restore(configured));
    let restored = table.state(FlowObjectId::from_test(10), flow).unwrap();
    assert_eq!(restored.requirement_entries().next().unwrap().1.len(), 2);

    assert!(table.restore(base));
    let restored = table.state(FlowObjectId::from_test(10), flow).unwrap();
    assert_eq!(restored.requirement_entries().count(), 0);
    assert_eq!(restored.sink_entries().count(), 0);

    assert!(table.record_requirement(
        FlowObjectId::from_test(10),
        flow,
        RequirementIndex::new(1).unwrap(),
        FactId::from_test(7),
    ));
    assert!(table.restore(base));
    assert_eq!(
        table
            .state(FlowObjectId::from_test(10), flow)
            .unwrap()
            .requirement_entries()
            .count(),
        0
    );
}
