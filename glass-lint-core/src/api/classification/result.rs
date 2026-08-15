use super::{ClassificationEvidence, MatchKind, MatchedCapability};
use crate::api::rule::Severity;

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
