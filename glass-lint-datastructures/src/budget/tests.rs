use super::*;

#[test]
fn rejects_overflow_and_records_exhaustion() {
    let mut budget = Budget::new(2);
    assert!(budget.try_push());
    assert!(budget.try_add(1));
    assert!(!budget.try_push());
    assert!(budget.exhausted());
}

#[test]
fn tracker_preserves_nested_pass_exhaustion() {
    let tracker = BudgetTracker::default();
    assert!(!tracker.is_exhausted());
    tracker.mark_exhausted();
    assert!(tracker.is_exhausted());
}

#[test]
fn remaining_decreases_with_charges() {
    let mut budget = Budget::new(5);
    assert_eq!(budget.remaining(), 5);
    budget.try_push();
    assert_eq!(budget.remaining(), 4);
    budget.try_add(3);
    assert_eq!(budget.remaining(), 1);
}

#[test]
fn remaining_is_zero_when_exhausted() {
    let mut budget = Budget::new(2);
    budget.try_push();
    budget.try_push();
    assert!(!budget.try_push());
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn remaining_does_not_underflow_on_overflow() {
    let mut budget = Budget::new(5);
    assert!(!budget.try_add(usize::MAX));
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn exhaustion_sticks_after_overflow() {
    let mut budget = Budget::new(10);
    assert!(!budget.try_add(usize::MAX));
    assert!(budget.exhausted());
    assert!(!budget.try_push());
}

#[test]
fn try_add_is_atomic_on_failure() {
    let mut budget = Budget::new(3);
    assert!(!budget.try_add(5));
    assert_eq!(budget.used(), 0);
}

#[test]
fn try_add_zero_always_succeeds_when_not_exhausted() {
    let mut budget = Budget::new(5);
    assert!(budget.try_add(0));
    assert_eq!(budget.used(), 0);
}

#[test]
fn try_add_zero_fails_when_exhausted() {
    let mut budget = Budget::new(1);
    budget.try_push();
    budget.try_push();
    assert!(!budget.try_add(0));
}

#[test]
fn try_push_on_exhausted_budget() {
    let mut budget = Budget::new(1);
    budget.try_push();
    assert!(!budget.try_push());
}

#[test]
fn budget_is_copy() {
    let a = Budget::new(10);
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn used_reports_correctly() {
    let mut budget = Budget::new(10);
    assert_eq!(budget.used(), 0);
    budget.try_add(3);
    assert_eq!(budget.used(), 3);
    budget.try_add(2);
    assert_eq!(budget.used(), 5);
}

#[test]
fn new_with_zero_limit() {
    let mut budget = Budget::new(0);
    assert!(!budget.try_push());
    assert!(budget.exhausted());
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn budget_tracker_default_is_not_exhausted() {
    let tracker = BudgetTracker::default();
    assert!(!tracker.is_exhausted());
}

#[test]
fn tracker_idempotent_mark_exhausted() {
    let tracker = BudgetTracker::default();
    tracker.mark_exhausted();
    tracker.mark_exhausted();
    assert!(tracker.is_exhausted());
}
