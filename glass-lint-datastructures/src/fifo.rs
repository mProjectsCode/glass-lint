use std::collections::{BTreeSet, VecDeque};

/// Result of admitting an item into a bounded deduplicating FIFO.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FifoAdmission {
    /// The item was new and is now pending.
    Inserted,
    /// An equal item was already retained.
    Duplicate,
    /// The item was new but the total-retained bound was reached.
    Full,
}

/// A deterministic FIFO bounded by its total number of unique retained items.
///
/// Popped items remain in the deduplication set, so the bound covers both the
/// pending frontier and items already processed. Reaching the bound latches
/// exhaustion and rejects subsequent new items.
pub struct BoundedFifo<T> {
    queue: VecDeque<T>,
    seen: BTreeSet<T>,
    max_retained: usize,
    exhausted: bool,
}

impl<T: Ord + Clone> BoundedFifo<T> {
    pub fn new(max_retained: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            seen: BTreeSet::new(),
            max_retained,
            exhausted: false,
        }
    }

    pub fn push(&mut self, entry: T) -> FifoAdmission {
        if self.seen.contains(&entry) {
            return FifoAdmission::Duplicate;
        }
        if self.seen.len() >= self.max_retained {
            self.exhausted = true;
            return FifoAdmission::Full;
        }
        self.seen.insert(entry.clone());
        self.queue.push_back(entry);
        FifoAdmission::Inserted
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn take_pending(&mut self) -> Vec<T> {
        std::mem::take(&mut self.queue).into_iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn retained_len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedFifo, FifoAdmission};

    #[test]
    fn deduplicates_and_counts_popped_items_toward_bound() {
        let mut fifo = BoundedFifo::new(1);
        assert_eq!(fifo.push(1), FifoAdmission::Inserted);
        assert_eq!(fifo.pop_front(), Some(1));
        assert_eq!(fifo.push(1), FifoAdmission::Duplicate);
        assert_eq!(fifo.push(2), FifoAdmission::Full);
        assert!(fifo.is_exhausted());
        assert_eq!(fifo.retained_len(), 1);
    }

    #[test]
    fn take_pending_preserves_fifo_order() {
        let mut fifo = BoundedFifo::new(3);
        assert_eq!(fifo.push(2), FifoAdmission::Inserted);
        assert_eq!(fifo.push(1), FifoAdmission::Inserted);
        assert_eq!(fifo.take_pending(), vec![2, 1]);
        assert!(fifo.is_empty());
    }
}
