use std::cell::Cell;

pub const UNLIMITED_SEMANTIC_OPS: usize = usize::MAX;

#[derive(Debug)]
pub struct SemanticBudget {
    used: Cell<usize>,
    limit: usize,
    exhausted: Cell<bool>,
}

impl SemanticBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            used: Cell::new(0),
            limit,
            exhausted: Cell::new(false),
        }
    }

    pub fn try_charge(&self) -> bool {
        if self.exhausted.get() {
            return false;
        }
        let used = self.used.get();
        let Some(next) = used.checked_add(1) else {
            self.exhausted.set(true);
            return false;
        };
        if next > self.limit {
            self.exhausted.set(true);
            return false;
        }
        self.used.set(next);
        true
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted.get()
    }

    pub fn used(&self) -> usize {
        self.used.get()
    }

    #[allow(dead_code)]
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for SemanticBudget {
    fn default() -> Self {
        Self::new(UNLIMITED_SEMANTIC_OPS)
    }
}
