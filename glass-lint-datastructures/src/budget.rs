use std::cell::Cell;

/// Tracks whether an analysis budget has been exhausted.
///
/// `BudgetTracker` uses interior mutability so it can be flagged as exhausted
/// through a shared reference (e.g. inside a `Cell` or `Arc`).  This lets
/// deeply nested analysis bail out without plumbing mutable ownership through
/// every call site.
#[derive(Debug, Default)]
pub struct BudgetTracker {
    exhausted: Cell<bool>,
}

impl BudgetTracker {
    /// Marks the budget as exhausted.
    pub fn mark_exhausted(&self) {
        self.exhausted.set(true);
    }

    /// Returns `true` if the budget has been exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted.get()
    }
}

/// A bounded consumption counter.
///
/// `Budget` tracks how many units have been consumed against a fixed limit.
/// Once exhausted (by exceeding the limit or overflowing `usize`), every
/// subsequent `try_push` / `try_add` returns `false` and the budget stays
/// exhausted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    limit: usize,
    used: usize,
    exhausted: bool,
}

impl Budget {
    /// Creates a new budget with the given `limit`.
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            exhausted: false,
        }
    }

    /// Consumes one unit.  Shorthand for `try_add(1)`.
    pub fn try_push(&mut self) -> bool {
        self.try_add(1)
    }

    /// Consumes `amount` units.
    ///
    /// Returns `true` on success.  Returns `false` (and marks the budget
    /// exhausted) when the addition would exceed the limit or overflow.
    ///
    /// The budget state is **not** updated on failure, so `used` stays at its
    /// previous value.
    pub fn try_add(&mut self, amount: usize) -> bool {
        if self.exhausted {
            return false;
        }
        let Some(next) = self.used.checked_add(amount) else {
            self.exhausted = true;
            return false;
        };
        if next > self.limit {
            self.exhausted = true;
            return false;
        }
        self.used = next;
        true
    }

    /// Returns `true` if the budget has been exhausted.
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    /// Returns the number of units consumed so far.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Returns the remaining capacity.
    ///
    /// Returns 0 if the budget is exhausted, even if `limit - used` would be
    /// positive.
    pub fn remaining(&self) -> usize {
        if self.exhausted {
            0
        } else {
            self.limit.saturating_sub(self.used)
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn budget_tracker_mark_exhausted_then_is_exhausted() {
        let tracker = BudgetTracker::default();
        tracker.mark_exhausted();
        assert!(tracker.is_exhausted());
    }

    #[test]
    fn budget_tracker_stays_exhausted() {
        let tracker = BudgetTracker::default();
        tracker.mark_exhausted();
        tracker.mark_exhausted();
        assert!(tracker.is_exhausted());
    }
}
