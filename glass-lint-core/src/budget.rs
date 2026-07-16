//! Small bounded counters shared by semantic subsystems.
//!
//! A budget deliberately exposes only checked operations. Callers must make
//! the exhaustion decision at the point where they would otherwise retain
//! partial state.

use std::cell::Cell;

/// Interior-mutable exhaustion state for passes that discover their bound in
/// a nested projector or linker helper.
#[derive(Debug, Default)]
pub struct BudgetTracker {
    exhausted: Cell<bool>,
}

impl BudgetTracker {
    /// Permanently record exhaustion for a nested pass.
    pub fn mark_exhausted(&self) {
        self.exhausted.set(true);
    }

    /// Whether any nested operation has exhausted this tracker.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Monotonic operation counter with a hard upper bound.
pub struct Budget {
    /// Maximum permitted operation count.
    limit: usize,
    /// Operations successfully charged so far.
    used: usize,
    /// Whether an attempted charge exceeded the bound.
    exhausted: bool,
}

impl Budget {
    /// Create an unused budget with the supplied limit.
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            exhausted: false,
        }
    }

    /// Charge one operation if capacity remains.
    pub fn try_push(&mut self) -> bool {
        self.try_add(1)
    }

    /// Charge several operations atomically, failing closed on overflow.
    pub fn try_add(&mut self, amount: usize) -> bool {
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

    /// Whether a charge has failed.
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, BudgetTracker};

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
}
