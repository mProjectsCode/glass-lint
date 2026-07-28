use hashbrown::HashMap;

use crate::analysis::value::ValueId;

/// A map supporting cheap snapshot/rollback via change logging.
///
/// Instead of cloning the entire HashMap at every control-flow branch point,
/// [`checkpoint`](OriginMap::checkpoint) records the current log position
/// (O(1)), and [`rollback`](OriginMap::rollback) undoes only the entries that
/// were actually modified since that checkpoint (O(changed entries)).
pub(in crate::analysis) struct OriginMap<V> {
    map: HashMap<ValueId, V>,
    log: Vec<LogEntry<V>>,
}

enum LogEntry<V> {
    Upsert { key: ValueId, had_old: Option<V> },
}

impl<V: Clone> OriginMap<V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            log: Vec::new(),
        }
    }

    /// Record a checkpoint. Returns the current log position.
    pub fn checkpoint(&self) -> usize {
        self.log.len()
    }

    /// Undo all mutations since `checkpoint`.
    pub fn rollback(&mut self, checkpoint: usize) {
        while self.log.len() > checkpoint {
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

    /// Clone the underlying map for callers that need a full immutable
    /// snapshot (e.g. retain_common).  The caller is responsible for bounding
    /// the number of snapshot calls.
    pub fn snapshot(&self) -> HashMap<ValueId, V> {
        self.map.clone()
    }

    pub fn get(&self, key: ValueId) -> Option<&V> {
        self.map.get(&key)
    }

    pub fn insert(&mut self, key: ValueId, value: V) {
        let had_old = self.map.insert(key, value);
        self.log.push(LogEntry::Upsert { key, had_old });
    }

    pub fn remove(&mut self, key: ValueId) {
        if let Some(old) = self.map.remove(&key) {
            self.log.push(LogEntry::Upsert {
                key,
                had_old: Some(old),
            });
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ValueId, &V)> {
        self.map.iter()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
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
        }
    }
}

impl<V: std::fmt::Debug> std::fmt::Debug for OriginMap<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginMap")
            .field("map", &self.map)
            .field("log.len", &self.log.len())
            .finish()
    }
}

impl<V: Clone> Clone for OriginMap<V> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            log: Vec::new(),
        }
    }
}
