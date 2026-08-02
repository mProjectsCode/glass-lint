use std::collections::BTreeMap;

use glass_lint_core::project::Finding;

#[derive(Clone, Debug)]
pub struct AdapterRun {
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CaseResult {
    pub id: String,
    pub description: String,
    pub source: String,
    pub adapters: BTreeMap<String, ToolResult>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ToolResult {
    pub version: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub passed: bool,
    pub findings: Vec<Finding>,
    pub mismatches: Vec<String>,
    pub operational_errors: Vec<String>,
}

impl ToolResult {
    #[must_use]
    pub fn skipped(version: String, skip_reason: Option<String>) -> Self {
        Self {
            version,
            skipped: true,
            skip_reason,
            passed: true,
            findings: vec![],
            mismatches: vec![],
            operational_errors: vec![],
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SuiteReport {
    pub schema_version: u32,
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.cases
            .iter()
            .all(|case| case.adapters.values().all(|adapter| adapter.passed))
    }
}
