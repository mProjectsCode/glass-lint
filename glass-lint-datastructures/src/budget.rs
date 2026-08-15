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
mod tests;
