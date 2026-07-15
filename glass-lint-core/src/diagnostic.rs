use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RuleId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
/// Severity exposed by the provider-neutral report schema.
pub enum Severity {
    Info,
    Warning,
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
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
/// Inclusive-start, exclusive-end source range used by findings.
pub struct SourceRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// Human-readable evidence attached to a finding.
pub struct Evidence {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// One rule finding with its primary range and optional evidence.
pub struct Finding {
    pub rule_id: RuleId,
    pub message_id: String,
    pub message: String,
    pub severity: Severity,
    pub range: SourceRange,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
/// Complete serialized result for one lint invocation.
pub struct LintReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub findings: Vec<Finding>,
    pub parse_diagnostics: Vec<crate::parse::ParseDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// Provider rule metadata exposed to front ends and integrations.
pub struct RuleMetadata {
    pub id: RuleId,
    pub description: String,
    pub default_severity: Severity,
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}
