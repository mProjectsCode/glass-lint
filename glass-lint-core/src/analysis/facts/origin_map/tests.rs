use crate::analysis::{
    SemanticBudget,
    facts::origin_map::{OriginMap, OriginSnapshot},
    model::value::ValueId,
};

#[test]
fn restoring_snapshot_rebases_owned_checkpoint() {
    let mut origins = OriginMap::<u8>::new();
    let mut checkpoint = origins.checkpoint();

    origins.restore_snapshot(
        OriginSnapshot {
            map: hashbrown::HashMap::new(),
        },
        &mut checkpoint,
        &SemanticBudget::default(),
    );

    assert!(checkpoint.active);
    assert_eq!(origins.open_checkpoints, 1);
    origins.commit(&mut checkpoint);
    assert!(!checkpoint.active);
    assert_eq!(origins.open_checkpoints, 0);
}

#[test]
fn restoring_snapshot_preserves_an_outer_checkpoint() {
    let budget = SemanticBudget::default();
    let mut origins = OriginMap::<u8>::new();
    let value = ValueId::new(1);
    origins.insert(value, 10, &budget);

    let outer = origins.checkpoint();
    origins.insert(value, 20, &budget);
    let mut inner = origins.checkpoint();
    origins.insert(value, 30, &budget);
    let snapshot = origins.snapshot(&budget);
    origins.insert(value, 40, &budget);

    origins.restore_snapshot(snapshot, &mut inner, &budget);
    assert_eq!(origins.get(value), Some(&30));
    origins.commit(&mut inner);

    origins.restore(&outer);
    assert_eq!(origins.get(value), Some(&10));
}

#[test]
fn branch_snapshot_intersects_only_changed_values() {
    let budget = SemanticBudget::default();
    let mut origins = OriginMap::<u8>::new();
    let unchanged = ValueId::new(1);
    let same_change = ValueId::new(2);
    let different_change = ValueId::new(3);
    let then_only_removal = ValueId::new(4);
    let else_only_change = ValueId::new(5);
    let reverted_else_change = ValueId::new(6);

    for (key, value) in [
        (unchanged, 10),
        (same_change, 20),
        (different_change, 30),
        (then_only_removal, 40),
        (else_only_change, 50),
        (reverted_else_change, 60),
    ] {
        origins.insert(key, value, &budget);
    }

    let mut checkpoint = origins.checkpoint();
    origins.insert(same_change, 21, &budget);
    origins.insert(different_change, 31, &budget);
    origins.remove(then_only_removal, &budget);
    let then = origins.branch_snapshot(&checkpoint, &budget);

    origins.restore(&checkpoint);
    origins.insert(same_change, 21, &budget);
    origins.insert(different_change, 32, &budget);
    origins.insert(else_only_change, 51, &budget);
    origins.insert(reverted_else_change, 61, &budget);
    origins.insert(reverted_else_change, 60, &budget);
    origins.retain_common_branch(&then, &checkpoint, &budget);
    origins.commit(&mut checkpoint);

    assert_eq!(origins.get(unchanged), Some(&10));
    assert_eq!(origins.get(same_change), Some(&21));
    assert_eq!(origins.get(different_change), None);
    assert_eq!(origins.get(then_only_removal), None);
    assert_eq!(origins.get(else_only_change), None);
    assert_eq!(origins.get(reverted_else_change), Some(&60));
}

#[test]
fn branch_snapshot_intersects_nested_full_replacements() {
    let budget = SemanticBudget::default();
    let mut origins = OriginMap::<u8>::new();
    let common = ValueId::new(1);
    let different = ValueId::new(2);
    origins.insert(common, 10, &budget);
    origins.insert(different, 20, &budget);

    let mut outer = origins.checkpoint();
    origins.insert(common, 11, &budget);
    origins.insert(different, 21, &budget);
    let mut then_inner = origins.checkpoint();
    let then_full = origins.snapshot(&budget);
    origins.insert(different, 99, &budget);
    origins.restore_snapshot(then_full, &mut then_inner, &budget);
    origins.commit(&mut then_inner);
    let then = origins.branch_snapshot(&outer, &budget);

    origins.restore(&outer);
    origins.insert(common, 11, &budget);
    origins.insert(different, 22, &budget);
    let mut else_inner = origins.checkpoint();
    let else_full = origins.snapshot(&budget);
    origins.insert(different, 98, &budget);
    origins.restore_snapshot(else_full, &mut else_inner, &budget);
    origins.commit(&mut else_inner);
    origins.retain_common_branch(&then, &outer, &budget);
    origins.commit(&mut outer);

    assert_eq!(origins.get(common), Some(&11));
    assert_eq!(origins.get(different), None);
}
