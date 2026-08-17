use std::collections::BTreeMap;

use glass_lint_core::project::Finding;

use crate::BundleKey;

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
    pub bundles: Vec<BundleResult>,
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

#[derive(Clone, Debug, serde::Serialize)]
pub struct BundleResult {
    pub key: BundleKey,
    pub transformer_version: Option<String>,
    pub passed: bool,
    pub authored_counts: BTreeMap<String, usize>,
    pub transformed_counts: BTreeMap<String, usize>,
    pub mismatches: Vec<String>,
    pub operational_errors: Vec<String>,
    pub generated_source_bytes: usize,
    pub generated_source_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_source: Option<String>,
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
        self.cases.iter().all(|case| {
            case.adapters.values().all(|adapter| adapter.passed)
                && case.bundles.iter().all(|bundle| bundle.passed)
        })
    }
}
