//! Serializable capability classifications and source evidence.
//!
//! Evidence keeps canonical fact spans and related cross-module events
//! separate. `rule_index` and event IDs are internal correlation keys and are
//! intentionally omitted from serialized reports.

use glass_lint_datastructures::ByteRange;

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
}

impl ClassificationEvidenceOccurrence {
    pub(crate) fn new(span: ByteRange, fact: Option<u32>, trace: Option<TraceNodeId>) -> Self {
        Self { span, fact, trace }
    }

    pub fn span(&self) -> ByteRange {
        self.span
    }

    pub fn fact(&self) -> Option<u32> {
        self.fact
    }

    pub(crate) fn trace(&self) -> Option<TraceNodeId> {
        self.trace
    }
}

impl ClassificationEvidence {
    pub(crate) fn from_occurrences(
        kind: MatchKind,
        symbol: String,
        occurrences: Vec<ClassificationEvidenceOccurrence>,
        certainty: MatchCertainty,
    ) -> Option<Self> {
        if occurrences.is_empty() {
            return None;
        }
        Some(Self {
            kind,
            symbol,
            count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
            truncated: false,
            certainty,
            occurrences,
        })
    }

    pub(crate) fn from_occurrence(
        kind: MatchKind,
        symbol: String,
        occurrence: ClassificationEvidenceOccurrence,
        certainty: MatchCertainty,
    ) -> Self {
        Self {
            kind,
            symbol,
            count: 1,
            truncated: false,
            certainty,
            occurrences: vec![occurrence],
        }
    }

    pub(crate) fn with_total_count(
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuleEvidenceTable {
    values: Vec<Vec<ClassificationEvidence>>,
}

impl RuleEvidenceTable {
    pub(crate) fn new(capacity: RuleEvidenceCapacity) -> Self {
        Self {
            values: (0..capacity.len()).map(|_| Vec::new()).collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(rule_count: usize) -> Self {
        Self::new(RuleEvidenceCapacity::from_catalog_len(rule_count))
    }

    pub(crate) fn for_rule(&self, rule: RuleIndex) -> Option<&[ClassificationEvidence]> {
        self.values.get(rule.get()).map(Vec::as_slice)
    }

    fn items_mut(
        &mut self,
        rule: RuleIndex,
    ) -> Result<&mut Vec<ClassificationEvidence>, RuleEvidenceError> {
        let capacity = self.values.len();
        self.values
            .get_mut(rule.get())
            .ok_or(RuleEvidenceError::RuleOutOfRange { rule, capacity })
    }

    pub(crate) fn record(
        &mut self,
        rule: RuleIndex,
        evidence: ClassificationEvidence,
    ) -> Result<(), RuleEvidenceError> {
        self.items_mut(rule)?.push(evidence);
        Ok(())
    }

    pub(crate) fn extend(
        &mut self,
        rule: RuleIndex,
        evidence: impl IntoIterator<Item = ClassificationEvidence>,
    ) -> Result<(), RuleEvidenceError> {
        self.items_mut(rule)?.extend(evidence);
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

    pub(crate) fn merge(&mut self, other: Self) -> Result<(), RuleEvidenceError> {
        if self.values.len() != other.values.len() {
            return Err(RuleEvidenceError::CapacityMismatch {
                expected: self.values.len(),
                actual: other.values.len(),
            });
        }
        for (rule, items) in other.values.into_iter().enumerate() {
            self.extend(RuleIndex::new(rule), items)?;
        }
        Ok(())
    }

    pub(crate) fn mark_all_possible(&mut self) {
        for items in &mut self.values {
            for evidence in items {
                evidence.mark_possible();
            }
        }
    }
}

#[cfg(test)]
mod test_indexing {
    use std::ops::{Index, IndexMut};

    use super::{ClassificationEvidence, RuleEvidenceTable};

    impl Index<usize> for RuleEvidenceTable {
        type Output = Vec<ClassificationEvidence>;

        fn index(&self, index: usize) -> &Self::Output {
            &self.values[index]
        }
    }

    impl IndexMut<usize> for RuleEvidenceTable {
        fn index_mut(&mut self, index: usize) -> &mut Self::Output {
            &mut self.values[index]
        }
    }
}

#[cfg(test)]
mod test_evidence_capacity {
    use glass_lint_datastructures::ByteRange;

    use super::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, MatchCertainty, MatchKind,
        RuleEvidenceError, RuleEvidenceTable, RuleIndex,
    };

    fn evidence() -> ClassificationEvidence {
        ClassificationEvidence {
            kind: MatchKind::Call,
            symbol: "fetch".to_owned(),
            count: 1,
            truncated: false,
            certainty: MatchCertainty::Definite,
            occurrences: vec![ClassificationEvidenceOccurrence {
                span: ByteRange::empty(),
                fact: None,
                trace: None,
            }],
        }
    }

    #[test]
    fn rejects_rule_indices_outside_catalog_capacity() {
        let mut table = RuleEvidenceTable::new_for_test(1);

        assert_eq!(
            table.record(RuleIndex::new(1), evidence()),
            Err(RuleEvidenceError::RuleOutOfRange {
                rule: RuleIndex::new(1),
                capacity: 1,
            })
        );
    }

    #[test]
    fn rejects_merging_tables_with_different_capacities() {
        let mut table = RuleEvidenceTable::new_for_test(1);
        let other = RuleEvidenceTable::new_for_test(2);

        assert_eq!(
            table.merge(other),
            Err(RuleEvidenceError::CapacityMismatch {
                expected: 1,
                actual: 2,
            })
        );
    }

    #[test]
    fn evidence_constructors_preserve_count_and_occurrence_invariants() {
        let occurrence = ClassificationEvidenceOccurrence::new(ByteRange::empty(), Some(1), None);
        assert!(
            ClassificationEvidence::from_occurrences(
                MatchKind::Call,
                "fetch".into(),
                Vec::new(),
                MatchCertainty::Definite,
            )
            .is_none()
        );
        assert!(
            ClassificationEvidence::with_total_count(
                MatchKind::Call,
                "fetch".into(),
                0,
                false,
                MatchCertainty::Definite,
                vec![occurrence],
            )
            .is_none()
        );

        let mut evidence = ClassificationEvidence::from_occurrences(
            MatchKind::Call,
            "fetch".into(),
            vec![occurrence],
            MatchCertainty::Definite,
        )
        .unwrap();
        evidence.mark_truncated();
        assert_eq!(evidence.count(), 1);
        assert!(evidence.is_truncated());

        let direct = ClassificationEvidence::from_occurrence(
            MatchKind::Call,
            "fetch".into(),
            occurrence,
            MatchCertainty::Possible,
        );
        assert_eq!(direct.count(), 1);
        assert_eq!(direct.occurrences(), &[occurrence]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Semantic kind of API occurrence represented in a report.
pub enum MatchKind {
    /// A callable symbol invocation.
    Call,
    /// Invocation of a member chain.
    MemberCall,
    /// Non-call member access.
    MemberRead,
    /// Assignment to a member property.
    PropertyWrite,
    /// A module import occurrence.
    Import,
    /// A matched static string occurrence.
    StringContains,
    /// A matched class declaration/use.
    Class,
    /// A constructor invocation/use.
    Constructor,
    /// Evidence attached to a call argument.
    CallArgument,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Top-level classification result containing capabilities in catalog order.
pub struct ClassificationResult {
    /// Classified capabilities selected for this run.
    capabilities: Vec<MatchedCapability>,
}

impl ClassificationResult {
    pub(crate) fn push_capability(&mut self, capability: MatchedCapability) {
        self.capabilities.push(capability);
    }

    /// Borrow the classified capabilities without copying them.
    pub fn capabilities(&self) -> &[MatchedCapability] {
        &self.capabilities
    }
}

impl MatchKind {
    /// Return the stable serialized spelling of this occurrence kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::MemberCall => "member_call",
            Self::MemberRead => "member_read",
            Self::PropertyWrite => "property_write",
            Self::Import => "import",
            Self::StringContains => "string_contains",
            Self::Class => "class",
            Self::Constructor => "constructor",
            Self::CallArgument => "call_argument",
        }
    }
}

impl MatchedCapability {
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

impl ClassificationEvidence {
    /// Return the occurrence kind.
    pub fn kind(&self) -> MatchKind {
        self.kind
    }

    /// Borrow the canonical matched symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
}
