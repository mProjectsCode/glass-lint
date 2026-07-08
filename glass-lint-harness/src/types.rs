use std::collections::BTreeMap;

use glass_lint_core::{Finding, Severity};
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct Case {
    pub id: String,
    pub description: String,
    pub tags: Vec<String>,
    pub language: String,
    pub filename: String,
    pub source: String,
    pub tools: BTreeMap<String, ToolExpectation>,
}

#[derive(Clone, Debug)]
pub struct ToolExpectation {
    pub rules: Vec<String>,
    pub required: Vec<DiagnosticExpectation>,
    pub forbidden: Vec<DiagnosticExpectation>,
}

#[derive(Clone, Debug)]
pub struct DiagnosticExpectation {
    pub rule_id: String,
    pub message_id: Option<String>,
    pub severity: Option<Severity>,
    pub count: Option<usize>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterRequest {
    pub protocol_version: u32,
    pub case_id: String,
    pub filename: String,
    pub source: String,
    pub rules: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdapterResponse {
    pub protocol_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub description: String,
    pub source: String,
    pub tools: BTreeMap<String, ToolResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub version: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub passed: bool,
    pub findings: Vec<Finding>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SuiteReport {
    pub schema_version: u32,
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    pub fn passed(&self) -> bool {
        self.cases
            .iter()
            .all(|case| case.tools.values().all(|tool| tool.passed))
    }
}
