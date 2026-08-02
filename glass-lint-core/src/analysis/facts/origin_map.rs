use hashbrown::HashMap;

use crate::analysis::{SemanticBudget, value::ValueId};

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

/// Single-use transaction token for one origin-map branch.
#[derive(Debug)]
pub(in crate::analysis) struct OriginCheckpoint {
    position: usize,
    active: bool,
}

enum LogEntry<V> {
    Upsert { key: ValueId, had_old: Option<V> },
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

    /// Clone the underlying map for callers that need a full immutable
    /// snapshot (e.g. retain_common).  The caller is responsible for bounding
    /// the number of snapshot calls.
    pub fn snapshot(&self, budget: &SemanticBudget) -> HashMap<ValueId, V> {
        budget.try_charge();
        self.map.clone()
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

    pub fn iter(&self) -> impl Iterator<Item = (&ValueId, &V)> {
        self.map.iter()
    }
}

impl<V: Clone> Default for OriginMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> From<HashMap<ValueId, V>> for OriginMap<V> {
    fn from(map: HashMap<ValueId, V>) -> Self {
        Self {
            map,
            log: Vec::new(),
            open_checkpoints: 0,
        }
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

impl<V: Clone> Clone for OriginMap<V> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            log: Vec::new(),
            open_checkpoints: 0,
        }
    }
}
