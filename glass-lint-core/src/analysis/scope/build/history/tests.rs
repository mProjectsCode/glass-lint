use glass_lint_datastructures::NameTable;

use super::*;

#[test]
fn assignment_checkpoints_still_restore_values() {
    let mut names = NameTable::default();
    let name = names.intern("value").unwrap();
    let scope = ScopeId::from_test(1);
    let mut environment = AssignmentEnvironment::new();
    environment.record_known(scope, name, BindingProvenance::Local);
    let base = environment.checkpoint();
    environment.record_unknown(scope, name);
    environment
        .restore(base)
        .expect("same assignment history accepts its checkpoint");
    assert_eq!(
        environment
            .get_by_id(scope, name)
            .map(|value| value.complete_witnesses().collect::<Vec<_>>()),
        Some(vec![&BindingProvenance::Local])
    );
}

#[test]
fn assignment_checkpoint_rejects_a_foreign_history() {
    let first = AssignmentEnvironment::new();
    let mut second = AssignmentEnvironment::new();

    assert_eq!(
        second.restore(first.checkpoint()),
        Err(HistoryRestoreError::ForeignCheckpoint)
    );
}

#[test]
fn write_set_restores_branch_local_deltas() {
    let mut writes = WriteSet::new();
    let mut names = NameTable::default();
    let first_name = names.intern("first").unwrap();
    let second_name = names.intern("second").unwrap();
    let first = ScopedName::new(ScopeId::from_test(1), first_name);
    let second = ScopedName::new(ScopeId::from_test(1), second_name);
    writes.insert(first.clone());
    let base = writes.checkpoint();
    writes.clear();
    writes.insert(second.clone());
    let branch = writes.checkpoint();

    writes
        .restore(base)
        .expect("same write history accepts its checkpoint");
    assert_eq!(writes.iter().collect::<Vec<_>>(), vec![first]);
    writes
        .restore(branch)
        .expect("same write history accepts its checkpoint");
    assert_eq!(writes.iter().collect::<Vec<_>>(), vec![second]);
}

#[test]
fn write_checkpoint_rejects_a_foreign_history() {
    let first = WriteSet::new();
    let mut second = WriteSet::new();

    assert_eq!(
        second.restore(first.checkpoint()),
        Err(HistoryRestoreError::ForeignCheckpoint)
    );
}
