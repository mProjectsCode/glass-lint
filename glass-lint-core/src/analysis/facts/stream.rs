//! Immutable semantic fact stream and deterministic event indexes.

use super::{ExactEventKey, FactId, FactKind, MAX_FACTS, SemanticFact};
use std::collections::BTreeMap;
use swc_common::BytePos;

#[derive(Debug)]
#[allow(dead_code)]
pub(in crate::analysis) struct FactStream {
    facts: Vec<SemanticFact>,
    exact: BTreeMap<ExactEventKey, Vec<FactId>>,
    exact_by_span_kind: BTreeMap<(BytePos, BytePos, FactKind), Vec<FactId>>,
    ordinal_counters: BTreeMap<(BytePos, BytePos, FactKind), u32>,
    valid: bool,
}

impl FactStream {
    pub(super) fn new() -> Self {
        Self {
            facts: Vec::new(),
            exact: BTreeMap::new(),
            exact_by_span_kind: BTreeMap::new(),
            ordinal_counters: BTreeMap::new(),
            valid: true,
        }
    }

    pub(super) fn push(&mut self, fact: SemanticFact) {
        if !self.valid || self.facts.len() >= MAX_FACTS {
            self.valid = false;
            return;
        }
        if fact.id.0 != self.facts.len() as u32 {
            self.valid = false;
            return;
        }
        let counter_key = (fact.span.lo(), fact.span.hi(), fact.kind);
        let ordinal = self
            .ordinal_counters
            .entry(counter_key)
            .and_modify(|o| {
                if let Some(next) = o.checked_add(1) {
                    *o = next;
                } else {
                    self.valid = false;
                }
            })
            .or_insert(0);
        if !self.valid {
            return;
        }
        let key = ExactEventKey {
            lo: fact.span.lo(),
            hi: fact.span.hi(),
            kind: fact.kind,
            ordinal: *ordinal,
        };
        self.exact.entry(key).or_default().push(fact.id);
        self.exact_by_span_kind
            .entry(counter_key)
            .or_default()
            .push(fact.id);
        self.facts.push(fact);
    }

    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }
    pub(super) fn is_valid(&self) -> bool {
        self.valid
    }
    #[allow(dead_code)]
    pub(super) fn exact_lookup(&self, key: &ExactEventKey) -> Vec<FactId> {
        self.exact.get(key).cloned().unwrap_or_default()
    }
    #[allow(dead_code)]
    pub(super) fn facts_at(&self, lo: BytePos, hi: BytePos, kind: FactKind) -> Vec<&SemanticFact> {
        self.exact_by_span_kind
            .get(&(lo, hi, kind))
            .into_iter()
            .flatten()
            .filter_map(|id| self.facts.get(id.0 as usize))
            .collect()
    }
    #[allow(dead_code)]
    pub(in crate::analysis) fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }
    #[cfg(test)]
    pub(super) fn fingerprint(&self) -> String {
        format!("{:?}", self.facts)
    }
}
