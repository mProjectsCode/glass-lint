use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RuleId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRange {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
pub struct LintReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub findings: Vec<Finding>,
    pub parse_diagnostics: Vec<crate::parse::ParseDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuleMetadata {
    pub id: RuleId,
    pub description: String,
    pub default_severity: Severity,
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}
