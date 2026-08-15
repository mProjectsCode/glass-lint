//! Parent-linked histories for bounded checkpoint and restore state.
//!
//! The history owns only the tree of positions. Callers own the meaning of a
//! delta and provide the inverse and forward operations when transitioning
//! between checkpoints. This keeps the storage reusable without making the
//! data-structures crate depend on analysis-specific state.

/// An opaque position in a [`ParentLinkedHistory`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryCursor(usize);

impl HistoryCursor {
    /// Return the position's stable numeric representation.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Direction of a delta while transitioning between history positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryTransition {
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
struct HistoryEntry<D> {
    parent: HistoryCursor,
    depth: usize,
    delta: D,
}

/// A parent-linked mutation history with O(1) checkpoints.
///
/// Recording after restoring creates a new branch while keeping previous
/// branches available as checkpoint targets. Moving between two positions
/// invokes `undo` for the source-only deltas and `redo` for the target-only
/// deltas, in the order required to reach the target state.
#[derive(Debug, Clone)]
pub struct ParentLinkedHistory<D> {
    entries: Vec<HistoryEntry<D>>,
    cursor: HistoryCursor,
}

impl<D> Default for ParentLinkedHistory<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> ParentLinkedHistory<D> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: HistoryCursor(0),
        }
    }

    /// Return the current position. Copying a cursor does not retain a borrow
    /// into the history, so callers can keep checkpoints across branches.
    pub const fn checkpoint(&self) -> HistoryCursor {
        self.cursor
    }

    /// Append a delta at the current position and make it the new position.
    pub fn record(&mut self, delta: D) -> HistoryCursor {
        let parent = self.cursor;
        let depth = self.depth(parent) + 1;
        self.entries.push(HistoryEntry {
            parent,
            depth,
            delta,
        });
        self.cursor = HistoryCursor(self.entries.len());
        self.cursor
    }

    /// Number of recorded entries, including entries on inactive branches.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history contains no recorded entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Move to `target`, applying only the deltas between the two positions.
    /// Returns `false` when the cursor does not belong to this history.
    pub fn transition(
        &mut self,
        target: HistoryCursor,
        mut apply: impl FnMut(HistoryTransition, &D),
    ) -> bool {
        if target.0 > self.entries.len() {
            return false;
        }

        let mut source = self.cursor;
        let mut destination = target;
        while self.depth(source) > self.depth(destination) {
            source = self.parent(source);
        }
        while self.depth(destination) > self.depth(source) {
            destination = self.parent(destination);
        }
        while source != destination {
            source = self.parent(source);
            destination = self.parent(destination);
        }
        let lca = source;

        let mut current = self.cursor;
        while current != lca {
            let entry = current.0 - 1;
            apply(HistoryTransition::Undo, &self.entries[entry].delta);
            current = self.entries[entry].parent;
        }

        let mut forward = Vec::new();
        current = target;
        while current != lca {
            let entry = current.0 - 1;
            forward.push(entry);
            current = self.entries[entry].parent;
        }
        for entry in forward.into_iter().rev() {
            apply(HistoryTransition::Redo, &self.entries[entry].delta);
        }

        self.cursor = target;
        true
    }

    fn depth(&self, cursor: HistoryCursor) -> usize {
        if cursor.0 == 0 {
            return 0;
        }
        self.entries[cursor.0 - 1].depth
    }

    fn parent(&self, cursor: HistoryCursor) -> HistoryCursor {
        if cursor.0 == 0 {
            HistoryCursor::default()
        } else {
            self.entries[cursor.0 - 1].parent
        }
    }
}

#[cfg(test)]
mod tests;
