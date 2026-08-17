use crate::{HistoryCursor, HistoryTransition, ParentLinkedHistory};

#[test]
fn transitions_between_branches_in_expected_order() {
    let mut history = ParentLinkedHistory::new();
    let root = history.checkpoint();
    history.record("a");
    let first_branch = history.checkpoint();
    history.record("b");
    let second_branch = history.checkpoint();
    let mut initial = Vec::new();
    history.transition(root, |direction, delta| {
        assert_eq!(direction, HistoryTransition::Undo);
        initial.push(*delta);
    });
    assert_eq!(initial, ["b", "a"]);
    history.record("c");

    let mut applied = Vec::new();
    assert!(history.transition(second_branch, |direction, delta| {
        applied.push(format!("{direction:?} {delta}"));
    },));
    assert_eq!(applied, ["Undo c", "Redo a", "Redo b"]);
    assert!(!history.transition(HistoryCursor::new_for_test(), |_, _| {}));
    assert_eq!(first_branch.index(), 1);
}

impl HistoryCursor {
    fn new_for_test() -> Self {
        Self(99)
    }
}
