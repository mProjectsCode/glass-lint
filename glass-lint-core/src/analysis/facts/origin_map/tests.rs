use super::{OriginMap, OriginSnapshot};

#[test]
fn restoring_snapshot_rebases_owned_checkpoint() {
    let mut origins = OriginMap::<u8>::new();
    let mut checkpoint = origins.checkpoint();

    origins.restore_snapshot(
        OriginSnapshot {
            map: hashbrown::HashMap::new(),
        },
        &mut checkpoint,
    );

    assert!(checkpoint.active);
    assert_eq!(origins.open_checkpoints, 1);
    origins.commit(&mut checkpoint);
    assert!(!checkpoint.active);
    assert_eq!(origins.open_checkpoints, 0);
}
