use swc_common::Span;

use super::rule::{ApiCategory, ApiSeverity, Confidence};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApiCapability {
    #[serde(skip)]
    pub(crate) rule_index: usize,
    pub id: String,
    pub label: String,
    pub category: ApiCategory,
    pub severity: ApiSeverity,
    pub confidence: Confidence,
    pub evidence: Vec<ApiEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApiEvidence {
    pub kind: ApiMatchKind,
    pub symbol: String,
    pub count: u32,
    #[serde(skip)]
    pub spans: Vec<Span>,
    /// Canonical fact IDs parallel to `spans`. `u32::MAX` is reserved for a
    /// deliberately synthetic occurrence that has no source fact.
    #[serde(skip)]
    pub(crate) event_ids: Vec<u32>,
    pub(crate) related: Vec<ApiRelatedEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ApiRelatedEvidence {
    pub(crate) module: u32,
    pub(crate) event: u32,
    pub(crate) kind: ApiMatchKind,
    pub(crate) symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiMatchKind {
    Call,
    MemberCall,
    MemberRead,
    Import,
    StringLiteral,
    Class,
    Constructor,
    CallArgument,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ApiClassificationResult {
    pub capabilities: Vec<ApiCapability>,
}

impl ApiClassificationResult {
    pub fn capabilities(&self) -> &[ApiCapability] {
        &self.capabilities
    }
}

impl ApiMatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::MemberCall => "member_call",
            Self::MemberRead => "member_read",
            Self::Import => "import",
            Self::StringLiteral => "string_literal",
            Self::Class => "class",
            Self::Constructor => "constructor",
            Self::CallArgument => "call_argument",
        }
    }
}

impl ApiCapability {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn severity(&self) -> ApiSeverity {
        self.severity
    }

    pub fn evidence(&self) -> &[ApiEvidence] {
        &self.evidence
    }
}

impl ApiEvidence {
    pub fn kind(&self) -> ApiMatchKind {
        self.kind
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }
}
