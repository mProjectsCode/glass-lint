//! Serializable capability classifications and source evidence.
//!
//! Evidence keeps canonical fact spans and related cross-module events
//! separate. `rule_index` and event IDs are internal correlation keys and are
//! intentionally omitted from serialized reports.

use super::rule::{Category, Confidence, Severity};
use crate::ByteRange;

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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
/// One classified capability emitted by a compiled matcher.
pub struct MatchedCapability {
    /// Internal catalog position used to correlate rule selections.
    #[serde(skip)]
    pub rule_index: RuleIndex,
    /// Stable namespaced rule ID.
    pub id: String,
    /// Human-readable capability label.
    pub label: String,
    /// Provider-owned category.
    pub category: Category,
    /// Severity assigned by the rule declaration.
    pub severity: Severity,
    /// Confidence assigned by the rule declaration.
    pub confidence: Confidence,
    /// Primary-file evidence for this capability.
    pub evidence: Vec<ClassificationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
/// Evidence for one matched API occurrence and its related events.
pub struct ClassificationEvidence {
    /// Semantic occurrence kind.
    pub kind: MatchKind,
    /// Canonical matched symbol/chain.
    pub symbol: String,
    /// Number of source events represented by this evidence item.
    pub count: u32,
    /// Whether the serialized occurrence list omits additional matches.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub evidence_truncated: bool,
    /// Primary occurrences with their optional canonical fact identity.
    #[serde(skip)]
    pub occurrences: Vec<ClassificationEvidenceOccurrence>,
    /// Related evidence from linked modules or flow projections.
    pub related: Vec<RelatedClassificationEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A source span and the fact that established it, when available.
pub struct ClassificationEvidenceOccurrence {
    pub span: ByteRange,
    pub fact: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
/// Cross-module evidence related to a primary occurrence.
pub struct RelatedClassificationEvidence {
    /// Stable project module ID containing the related event.
    pub module: u32,
    /// Canonical fact/event ID within that module.
    pub event: u32,
    /// Related occurrence kind.
    pub kind: MatchKind,
    /// Related matched symbol/chain.
    pub symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
/// Semantic kind of API occurrence represented in a report.
pub enum MatchKind {
    /// A callable symbol invocation.
    Call,
    /// Invocation of a member chain.
    MemberCall,
    /// Non-call member access.
    MemberRead,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
/// Top-level classification result containing capabilities in catalog order.
pub struct ClassificationResult {
    /// Classified capabilities selected for this run.
    pub capabilities: Vec<MatchedCapability>,
}

impl ClassificationResult {
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
