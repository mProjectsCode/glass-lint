use hashbrown::{HashMap, HashSet};

use crate::analysis::{SemanticBudget, model::value::ValueId};

/// A map supporting cheap snapshot/rollback via change logging.
///
/// Instead of cloning the entire HashMap at every control-flow branch point,
/// [`checkpoint`](OriginMap::checkpoint) records the current log position
/// (O(1)), and [`rollback`](OriginMap::rollback) undoes only the entries that
/// were actually modified since that checkpoint (O(changed entries)).
///
/// Mutations are only logged while at least one checkpoint is active. When
/// the last checkpoint is closed the entire log is discarded, keeping storage
/// bounded by the active delta. Every logged mutation and snapshot charges
/// the semantic budget before allocation.
pub(in crate::analysis) struct OriginMap<V> {
    map: HashMap<ValueId, V>,
    log: Vec<LogEntry<V>>,
    open_checkpoints: usize,
}

/// Opaque full-state capture of an [`OriginMap`] for control-flow joins.
///
/// Produced by [`snapshot`](OriginMap::snapshot), merged through
/// [`retain_common`](OriginMap::retain_common), and restored with
/// [`restore_snapshot`](OriginMap::restore_snapshot). The raw map stays private
/// so equality and branch-intersection rules remain on [`OriginMap`].
pub(in crate::analysis) struct OriginSnapshot<V> {
    map: HashMap<ValueId, V>,
}

/// Values changed by one branch relative to its owning checkpoint.
///
/// Unlike [`OriginSnapshot`], this does not duplicate entries that remain
/// untouched in both alternatives. Those entries are already known to be
/// common at the join because rollback restores the checkpoint state before
/// the second alternative is visited.
pub(in crate::analysis) struct OriginBranchSnapshot<V> {
    changes: HashMap<ValueId, Option<V>>,
}

/// Single-use transaction token for one origin-map branch.
#[derive(Debug)]
pub(in crate::analysis) struct OriginCheckpoint {
    position: usize,
    active: bool,
}

enum LogEntry<V> {
    Upsert { key: ValueId, had_old: Option<V> },
    Replace { old: HashMap<ValueId, V> },
}

impl<V: Clone> OriginMap<V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            log: Vec::new(),
            open_checkpoints: 0,
        }
    }

    /// Record a checkpoint. Returns the current log position.
    pub fn checkpoint(&mut self) -> OriginCheckpoint {
        self.open_checkpoints += 1;
        OriginCheckpoint {
            position: self.log.len(),
            active: true,
        }
    }

    /// Undo all mutations since `checkpoint`.
    pub fn restore(&mut self, checkpoint: &OriginCheckpoint) {
        if !checkpoint.active {
            return;
        }
        while self.log.len() > checkpoint.position {
            match self.log.pop().unwrap() {
                LogEntry::Upsert { key, had_old } => match had_old {
                    Some(old) => {
                        self.map.insert(key, old);
                    }
                    None => {
                        self.map.remove(&key);
                    }
                },
                LogEntry::Replace { old } => self.map = old,
            }
        }
    }

    /// Undo all mutations since `checkpoint` and close the transaction.
    pub fn rollback(&mut self, checkpoint: &mut OriginCheckpoint) {
        if !checkpoint.active {
            return;
        }
        self.restore(checkpoint);
        self.open_checkpoints = self.open_checkpoints.saturating_sub(1);
        if self.open_checkpoints == 0 {
            self.log.clear();
        }
        checkpoint.active = false;
    }

    /// Discard all log entries up to `checkpoint`. These entries belong to
    /// completed control regions whose mutations are now permanent; they will
    /// never need to be rolled back.
    pub fn commit(&mut self, checkpoint: &mut OriginCheckpoint) {
        if !checkpoint.active {
            return;
        }
        self.open_checkpoints = self.open_checkpoints.saturating_sub(1);
        if self.open_checkpoints == 0 {
            self.log.clear();
        }
        checkpoint.active = false;
    }

    /// Capture the full map contents as an opaque snapshot for a join point.
    /// The caller is responsible for bounding the number of snapshot calls.
    pub fn snapshot(&self, budget: &SemanticBudget) -> OriginSnapshot<V> {
        budget.try_charge();
        OriginSnapshot {
            map: self.map.clone(),
        }
    }

    /// Capture only values changed since `checkpoint` for a two-way branch.
    pub fn branch_snapshot(
        &self,
        checkpoint: &OriginCheckpoint,
        budget: &SemanticBudget,
    ) -> OriginBranchSnapshot<V>
    where
        V: PartialEq,
    {
        debug_assert!(checkpoint.active);
        debug_assert!(checkpoint.position <= self.log.len());
        budget.try_charge();

        let branch_log = &self.log[checkpoint.position..];
        if branch_log
            .iter()
            .any(|entry| matches!(entry, LogEntry::Replace { .. }))
        {
            return OriginBranchSnapshot {
                changes: self.changes_since(checkpoint),
            };
        }
        let mut changes = HashMap::with_capacity(branch_log.len());
        for entry in branch_log {
            if let LogEntry::Upsert { key, .. } = entry {
                changes
                    .entry(*key)
                    .or_insert_with(|| self.map.get(key).cloned());
            }
        }
        OriginBranchSnapshot { changes }
    }

    fn changes_since(&self, checkpoint: &OriginCheckpoint) -> HashMap<ValueId, Option<V>>
    where
        V: PartialEq,
    {
        let branch_log = &self.log[checkpoint.position..];
        let mut baseline = self.map.clone();
        for entry in branch_log.iter().rev() {
            match entry {
                LogEntry::Upsert { key, had_old } => match had_old {
                    Some(old) => {
                        baseline.insert(*key, old.clone());
                    }
                    None => {
                        baseline.remove(key);
                    }
                },
                LogEntry::Replace { old } => baseline.clone_from(old),
            }
        }

        let mut changes = HashMap::new();
        for (key, value) in &self.map {
            if baseline.get(key) != Some(value) {
                changes.insert(*key, Some(value.clone()));
            }
        }
        for key in baseline.keys() {
            if !self.map.contains_key(key) {
                changes.insert(*key, None);
            }
        }
        changes
    }

    /// Replace the full contents with `snapshot` and rebase its owning
    /// checkpoint at the new log position.
    ///
    /// Full restoration replaces the journal, so the checkpoint that owns the
    /// restored branch must be supplied and remains active against the new
    /// journal. This keeps its later commit or rollback balanced with the
    /// checkpoint count instead of silently invalidating the transaction.
    pub fn restore_snapshot(
        &mut self,
        snapshot: OriginSnapshot<V>,
        checkpoint: &mut OriginCheckpoint,
        budget: &SemanticBudget,
    ) {
        debug_assert!(checkpoint.active);
        self.restore(checkpoint);
        let old = std::mem::replace(&mut self.map, snapshot.map);
        if self.open_checkpoints > 1 {
            budget.try_charge();
            self.log.push(LogEntry::Replace { old });
        }
        checkpoint.position = self.log.len();
    }

    pub fn get(&self, key: ValueId) -> Option<&V> {
        self.map.get(&key)
    }

    pub fn insert(&mut self, key: ValueId, value: V, budget: &SemanticBudget) {
        let had_old = self.map.insert(key, value);
        if self.open_checkpoints > 0 {
            budget.try_charge();
            self.log.push(LogEntry::Upsert { key, had_old });
        }
    }

    pub fn remove(&mut self, key: ValueId, budget: &SemanticBudget) {
        if let Some(old) = self.map.remove(&key)
            && self.open_checkpoints > 0
        {
            budget.try_charge();
            self.log.push(LogEntry::Upsert {
                key,
                had_old: Some(old),
            });
        }
    }
}

impl<V: Clone + PartialEq> OriginMap<V> {
    /// Retain only entries whose value is identical to the one recorded in
    /// `other` for the same key, removing every other entry. This is the
    /// branch-intersection step at control-flow joins.
    pub fn retain_common(&mut self, other: &OriginSnapshot<V>, budget: &SemanticBudget) {
        let log_changes = self.open_checkpoints > 0;
        for (key, old) in self
            .map
            .extract_if(|value, origin| other.map.get(value) != Some(origin))
        {
            if log_changes {
                budget.try_charge();
                self.log.push(LogEntry::Upsert {
                    key,
                    had_old: Some(old),
                });
            }
        }
    }

    /// Intersect two branch alternatives using only the keys changed relative
    /// to their shared checkpoint.
    pub fn retain_common_branch(
        &mut self,
        other: &OriginBranchSnapshot<V>,
        checkpoint: &OriginCheckpoint,
        budget: &SemanticBudget,
    ) {
        debug_assert!(checkpoint.active);
        debug_assert!(checkpoint.position <= self.log.len());

        let branch_log = &self.log[checkpoint.position..];
        let mut to_remove = Vec::with_capacity(other.changes.len().min(self.map.len()));
        for (key, then_value) in &other.changes {
            if self.map.get(key) != then_value.as_ref() {
                to_remove.push(*key);
            }
        }

        if branch_log
            .iter()
            .any(|entry| matches!(entry, LogEntry::Replace { .. }))
        {
            let else_changes = self.changes_since(checkpoint);
            to_remove.extend(
                else_changes
                    .keys()
                    .filter(|key| !other.changes.contains_key(*key))
                    .copied(),
            );
            for key in to_remove {
                self.remove(key, budget);
            }
            return;
        }

        let mut visited_else = HashSet::with_capacity(branch_log.len());
        for entry in branch_log {
            if let LogEntry::Upsert { key, had_old } = entry {
                if other.changes.contains_key(key) || !visited_else.insert(*key) {
                    continue;
                }
                if self.map.get(key) != had_old.as_ref() {
                    to_remove.push(*key);
                }
            }
        }

        for key in to_remove {
            self.remove(key, budget);
        }
    }
}

impl<V: Clone> Default for OriginMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: std::fmt::Debug> std::fmt::Debug for OriginMap<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginMap")
            .field("map", &self.map)
            .field("log.len", &self.log.len())
            .field("open_checkpoints", &self.open_checkpoints)
            .finish()
    }
}

#[cfg(test)]
mod tests;
