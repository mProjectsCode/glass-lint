//! Serializable capability classifications and source evidence.
//!
//! Evidence keeps canonical fact spans and related cross-module events
//! separate. `rule_index` and event IDs are internal correlation keys and are
//! intentionally omitted from serialized reports. The occurrence-kind
//! vocabulary ([`MatchKind`]) lives with the rule declarations and is
//! re-exported here for serialization.

use std::collections::BTreeMap;

use glass_lint_datastructures::ByteRange;

pub use crate::api::rule::MatchKind;
use crate::{analysis::trace::TraceNodeId, api::rule::Severity, project::MatchCertainty};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Stable position of a rule within a validated catalog.
pub struct RuleIndex(usize);

impl RuleIndex {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleEvidenceCapacity(usize);

impl RuleEvidenceCapacity {
    pub(crate) const fn from_catalog_len(rule_count: usize) -> Self {
        Self(rule_count)
    }

    pub(crate) const fn len(self) -> usize {
        self.0
    }

    pub(crate) const fn contains(self, rule: RuleIndex) -> bool {
        rule.get() < self.0
    }

    pub(crate) fn validate(self, rule: RuleIndex) -> Result<(), RuleEvidenceError> {
        self.contains(rule)
            .then_some(())
            .ok_or(RuleEvidenceError::RuleOutOfRange {
                rule,
                capacity: self.0,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEvidenceError {
    RuleOutOfRange { rule: RuleIndex, capacity: usize },
    CapacityMismatch { expected: usize, actual: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One classified capability emitted by a compiled matcher.
pub struct MatchedCapability {
    /// Internal catalog position used to correlate rule selections.
    rule_index: RuleIndex,
    /// Human-readable capability label.
    label: String,
    /// Severity assigned by the rule declaration.
    severity: Severity,
    /// Primary-file evidence for this capability.
    evidence: Vec<ClassificationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Evidence for one matched API occurrence and its related events.
pub struct ClassificationEvidence {
    /// Semantic occurrence kind.
    kind: MatchKind,
    /// Canonical matched symbol/chain.
    symbol: String,
    /// Number of source events represented by this evidence item.
    count: u32,
    /// Whether the serialized occurrence list omits additional matches.
    truncated: bool,
    /// Whether the match holds on all or only some modeled paths.
    certainty: MatchCertainty,
    /// Primary occurrences with their optional canonical fact identity
    /// and trace head into the interned trace arena.
    occurrences: Vec<ClassificationEvidenceOccurrence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A source span, the fact that established it, and an optional trace head
/// into the interned trace arena.
pub struct ClassificationEvidenceOccurrence {
    span: ByteRange,
    fact: Option<u32>,
    /// Head of the evidence trace chain in the arena, if available.
    trace: Option<TraceNodeId>,
}

impl MatchedCapability {
    pub(crate) fn new(
        rule_index: RuleIndex,
        label: String,
        severity: Severity,
        evidence: Vec<ClassificationEvidence>,
    ) -> Self {
        Self {
            rule_index,
            label,
            severity,
            evidence,
        }
    }

    pub(crate) fn rule_index(&self) -> RuleIndex {
        self.rule_index
    }

    /// Borrow the capability label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Return the declared severity.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Borrow primary evidence for this capability.
    pub fn evidence(&self) -> &[ClassificationEvidence] {
        &self.evidence
    }
}

impl ClassificationEvidenceOccurrence {
    pub(crate) fn new(span: ByteRange, fact: Option<u32>, trace: Option<TraceNodeId>) -> Self {
        Self { span, fact, trace }
    }

    pub fn span(&self) -> ByteRange {
        self.span
    }

    pub(crate) fn fact(&self) -> Option<u32> {
        self.fact
    }

    pub(crate) fn trace(&self) -> Option<TraceNodeId> {
        self.trace
    }
}

impl ClassificationEvidence {
    pub(crate) fn from_parts(
        kind: MatchKind,
        symbol: String,
        total_count: usize,
        truncated: bool,
        certainty: MatchCertainty,
        occurrences: Vec<ClassificationEvidenceOccurrence>,
    ) -> Option<Self> {
        if total_count < occurrences.len() {
            return None;
        }
        Some(Self {
            kind,
            symbol,
            count: u32::try_from(total_count).unwrap_or(u32::MAX),
            truncated,
            certainty,
            occurrences,
        })
    }

    pub(crate) fn from_occurrences(
        kind: MatchKind,
        symbol: String,
        occurrences: Vec<ClassificationEvidenceOccurrence>,
        certainty: MatchCertainty,
    ) -> Option<Self> {
        if occurrences.is_empty() {
            return None;
        }
        Self::from_parts(
            kind,
            symbol,
            occurrences.len(),
            false,
            certainty,
            occurrences,
        )
    }

    pub(crate) fn from_occurrence(
        kind: MatchKind,
        symbol: String,
        occurrence: ClassificationEvidenceOccurrence,
        certainty: MatchCertainty,
    ) -> Self {
        Self::from_parts(kind, symbol, 1, false, certainty, vec![occurrence])
            .expect("one retained occurrence must fit its total count")
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub fn certainty(&self) -> MatchCertainty {
        self.certainty
    }

    pub fn occurrences(&self) -> &[ClassificationEvidenceOccurrence] {
        &self.occurrences
    }

    /// Return the occurrence kind.
    pub fn kind(&self) -> MatchKind {
        self.kind
    }

    /// Borrow the canonical matched symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub(crate) fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    pub(crate) fn mark_possible(&mut self) {
        self.certainty = MatchCertainty::Possible;
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        self.certainty = if self.certainty == MatchCertainty::Possible
            || other.certainty == MatchCertainty::Possible
        {
            MatchCertainty::Possible
        } else {
            MatchCertainty::Definite
        };
        self.count = self.count.saturating_add(other.count);
        self.truncated |= other.truncated;
        self.occurrences.append(&mut other.occurrences);
    }
}

/// Bounded evidence grouped by opaque catalog rule index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvidenceTable {
    capacity: RuleEvidenceCapacity,
    values: BTreeMap<RuleIndex, Vec<ClassificationEvidence>>,
}

impl RuleEvidenceTable {
    pub(crate) fn new(capacity: RuleEvidenceCapacity) -> Self {
        Self {
            capacity,
            values: BTreeMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(rule_count: usize) -> Self {
        Self::new(RuleEvidenceCapacity::from_catalog_len(rule_count))
    }

    pub(crate) fn for_rule(&self, rule: RuleIndex) -> Option<&[ClassificationEvidence]> {
        self.values.get(&rule).map(Vec::as_slice)
    }

    fn items_mut(
        &mut self,
        rule: RuleIndex,
    ) -> Result<&mut Vec<ClassificationEvidence>, RuleEvidenceError> {
        self.capacity.validate(rule)?;
        Ok(self.values.entry(rule).or_default())
    }

    pub(crate) fn record(
        &mut self,
        rule: RuleIndex,
        evidence: ClassificationEvidence,
    ) -> Result<(), RuleEvidenceError> {
        self.items_mut(rule)?.push(evidence);
        Ok(())
    }

    pub(crate) fn mark_event_truncated(
        &mut self,
        rule: RuleIndex,
        event: u32,
    ) -> Result<(), RuleEvidenceError> {
        let items = self.items_mut(rule)?;
        for evidence in items {
            if evidence
                .occurrences()
                .iter()
                .any(|occurrence| occurrence.fact() == Some(event))
            {
                evidence.mark_truncated();
            }
        }
        Ok(())
    }

    pub(crate) fn replace(
        &mut self,
        rule: RuleIndex,
        evidence: Vec<ClassificationEvidence>,
    ) -> Result<(), RuleEvidenceError> {
        *self.items_mut(rule)? = evidence;
        Ok(())
    }

    pub(crate) fn merge_equal_capacity(&mut self, other: Self) -> Result<(), RuleEvidenceError> {
        if self.capacity != other.capacity {
            return Err(RuleEvidenceError::CapacityMismatch {
                expected: self.capacity.len(),
                actual: other.capacity.len(),
            });
        }
        for (rule, other_items) in other.values {
            self.values.entry(rule).or_default().extend(other_items);
        }
        Ok(())
    }

    pub(crate) fn mark_all_possible(&mut self) {
        for items in self.values.values_mut() {
            for evidence in items {
                evidence.mark_possible();
            }
        }
    }
}

#[cfg(test)]
mod tests;

mod result;
pub use result::ClassificationResult;
