//! Provider-neutral diagnostic and serialized report data types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RuleId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
/// Severity exposed by the provider-neutral report schema.
pub enum Severity {
    /// Informational diagnostic.
    Info,
    /// Warning diagnostic.
    Warning,
    /// Error diagnostic.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Info => "info",
                Self::Warning => "warning",
                Self::Error => "error",
            }
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
/// One-based Unicode display position in a source file.
pub struct Position {
    /// One-based source line.
    pub line: u32,
    /// One-based Unicode display column.
    pub column: u32,
}

impl Position {
    /// Convert a byte offset into a clamped one-based display position.
    pub fn from_source(source: &str, offset: usize) -> Self {
        let mut end = offset.min(source.len());
        while end > 0 && !source.is_char_boundary(end) {
            end -= 1;
        }
        let prefix = &source[..end];

        Self {
            line: prefix
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_add(1),
            column: prefix
                .rsplit_once('\n')
                .map_or_else(|| prefix.chars().count(), |(_, tail)| tail.chars().count())
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
/// Inclusive-start, exclusive-end source range used by findings.
pub struct SourceRange {
    /// Inclusive start position.
    pub start: Position,
    /// Exclusive end position.
    pub end: Position,
}

impl SourceRange {
    /// Construct a range from a byte start and byte length.
    pub fn from_source(source: &str, start: usize, length: usize) -> Self {
        Self {
            start: Position::from_source(source, start),
            end: Position::from_source(source, start.saturating_add(length)),
        }
    }

    /// Whether this range fully contains another range.
    pub fn contains(&self, inner: &Self) -> bool {
        self.start <= inner.start && inner.end <= self.end
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// Human-readable evidence attached to a finding.
pub struct Evidence {
    /// Human-readable evidence message.
    pub message: String,
    /// Exact number of semantic matches represented by this evidence group.
    #[serde(default)]
    pub count: u32,
    /// Whether only the presentation evidence for this group was retained.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub evidence_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional source range for the evidence.
    pub range: Option<SourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional related source text.
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// One rule finding with its primary range and optional evidence.
pub struct Finding {
    /// Stable rule ID that produced the finding.
    pub rule_id: RuleId,
    /// Stable message identifier.
    pub message_id: String,
    /// Rendered finding message.
    pub message: String,
    /// Finding severity.
    pub severity: Severity,
    /// Primary source range.
    pub range: SourceRange,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// Complete serialized result for one lint invocation.
pub struct LintReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Tool/core version string.
    pub tool_version: String,
    /// Deterministically ordered findings.
    pub findings: Vec<Finding>,
    /// Parse diagnostics emitted before semantic analysis.
    pub parse_diagnostics: Vec<crate::parse::ParseDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// Provider rule metadata exposed to front ends and integrations.
pub struct RuleMetadata {
    /// Stable rule ID.
    pub id: RuleId,
    /// Provider-facing description.
    pub description: String,
    /// Default report severity.
    pub default_severity: Severity,
    /// Stable message templates keyed by message ID.
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}
