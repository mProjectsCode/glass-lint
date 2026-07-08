use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{Finding, LintReport, Severity};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

pub const ADAPTER_PROTOCOL_VERSION: u32 = 1;

fn default_language() -> String {
    "javascript".into()
}
fn default_filename() -> String {
    "main.js".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_filename")]
    pub filename: String,
    pub source: String,
    pub tools: BTreeMap<String, ToolExpectation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExpectation {
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default, rename = "expect")]
    pub required: Vec<DiagnosticExpectation>,
    #[serde(default, rename = "forbid")]
    pub forbidden: Vec<DiagnosticExpectation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticExpectation {
    #[serde(rename = "rule")]
    pub rule_id: String,
    pub message_id: Option<String>,
    pub severity: Option<Severity>,
    #[serde(default = "one")]
    pub count: usize,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: Option<String>,
}
fn one() -> usize {
    1
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

pub trait Adapter {
    fn name(&self) -> &str;
    fn version(&self) -> Result<String>;
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>>;
}

pub struct GlassLintAdapter;
impl Adapter for GlassLintAdapter {
    fn name(&self) -> &str {
        "glass-lint"
    }
    fn version(&self) -> Result<String> {
        Ok(env!("CARGO_PKG_VERSION").into())
    }
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        let configured = glass_lint_obsidian::heuristic_linter();
        let enabled = expectation
            .rules
            .iter()
            .map(|id| glass_lint_core::RuleId::parse(id.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let linter = glass_lint_core::Linter::with_rules(configured.catalog().clone(), enabled)?;
        let report = linter.lint(&case.source, &case.filename);
        if !report.parse_diagnostics.is_empty() {
            bail!(
                "{}",
                report
                    .parse_diagnostics
                    .into_iter()
                    .map(|d| d.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        Ok(report.findings)
    }
}

pub struct ExternalAdapter {
    pub name: String,
    pub command: PathBuf,
}
impl Adapter for ExternalAdapter {
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> Result<String> {
        Ok("external".into())
    }
    fn run(&self, case: &Case, expectation: &ToolExpectation) -> Result<Vec<Finding>> {
        let mut child = Command::new(&self.command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start adapter {}", self.command.display()))?;
        let request = AdapterRequest {
            protocol_version: ADAPTER_PROTOCOL_VERSION,
            case_id: case.id.clone(),
            filename: case.filename.clone(),
            source: case.source.clone(),
            rules: expectation.rules.clone(),
        };
        serde_json::to_writer(
            child.stdin.as_mut().context("adapter stdin unavailable")?,
            &request,
        )?;
        child.stdin.take().unwrap().flush()?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!(
                "adapter exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let response: AdapterResponse =
            serde_json::from_slice(&output.stdout).context("invalid adapter response")?;
        if response.protocol_version != ADAPTER_PROTOCOL_VERSION {
            bail!(
                "adapter protocol version {}, expected {}",
                response.protocol_version,
                ADAPTER_PROTOCOL_VERSION
            );
        }
        if response.tool != self.name {
            bail!(
                "adapter identified as `{}`, expected `{}`",
                response.tool,
                self.name
            );
        }
        Ok(response.findings)
    }
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

pub fn load_cases(root: &Path) -> Result<Vec<Case>> {
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "case.toml")
        .map(|entry| entry.into_path())
        .collect();
    paths.sort();
    let mut ids = BTreeSet::new();
    paths
        .into_iter()
        .map(|path| {
            let text =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let case: Case =
                toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
            if case.language != "javascript" {
                bail!(
                    "{}: unsupported language `{}`",
                    path.display(),
                    case.language
                );
            }
            if !ids.insert(case.id.clone()) {
                bail!("duplicate case id `{}`", case.id);
            }
            Ok(case)
        })
        .collect()
}

pub fn run_suite(root: &Path, adapters: &[Box<dyn Adapter>]) -> Result<SuiteReport> {
    let cases = load_cases(root)?;
    let registered: BTreeSet<_> = adapters
        .iter()
        .map(|adapter| adapter.name().to_owned())
        .collect();
    let mut results = Vec::new();
    for case in cases {
        let configured: BTreeSet<_> = case.tools.keys().cloned().collect();
        if configured != registered {
            let missing: Vec<_> = registered.difference(&configured).cloned().collect();
            let unknown: Vec<_> = configured.difference(&registered).cloned().collect();
            bail!(
                "case `{}` tool coverage mismatch (missing: {:?}, unknown: {:?})",
                case.id,
                missing,
                unknown
            );
        }
        let mut tools = BTreeMap::new();
        for adapter in adapters {
            let expectation = &case.tools[adapter.name()];
            let version = adapter
                .version()
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let (findings, errors) = match adapter.run(&case, expectation) {
                Ok(findings) => {
                    let errors = compare(&findings, expectation);
                    (findings, errors)
                }
                Err(error) => (vec![], vec![error.to_string()]),
            };
            tools.insert(
                adapter.name().into(),
                ToolResult {
                    version,
                    passed: errors.is_empty(),
                    findings,
                    errors,
                },
            );
        }
        results.push(CaseResult {
            id: case.id,
            description: case.description,
            source: case.source,
            tools,
        });
    }
    Ok(SuiteReport {
        schema_version: 1,
        cases: results,
    })
}

fn matches(finding: &Finding, expected: &DiagnosticExpectation) -> bool {
    finding.rule_id.as_str() == expected.rule_id
        && expected
            .message_id
            .as_ref()
            .is_none_or(|id| &finding.message_id == id)
        && expected
            .severity
            .is_none_or(|severity| finding.severity == severity)
        && expected
            .line
            .is_none_or(|line| finding.range.start.line == line)
        && expected
            .column
            .is_none_or(|column| finding.range.start.column == column)
        && expected
            .message
            .as_ref()
            .is_none_or(|message| &finding.message == message)
}

fn compare(findings: &[Finding], expectation: &ToolExpectation) -> Vec<String> {
    let mut errors = Vec::new();
    for expected in &expectation.required {
        let actual = findings
            .iter()
            .filter(|finding| matches(finding, expected))
            .count();
        if actual != expected.count {
            errors.push(format!(
                "expected {} × {}, found {}",
                expected.count, expected.rule_id, actual
            ));
        }
    }
    for forbidden in &expectation.forbidden {
        let actual = findings
            .iter()
            .filter(|finding| matches(finding, forbidden))
            .count();
        if actual > 0 {
            errors.push(format!(
                "forbidden diagnostic {} appeared {} time(s)",
                forbidden.rule_id, actual
            ));
        }
    }
    for finding in findings {
        if !expectation
            .required
            .iter()
            .any(|expected| matches(finding, expected))
        {
            errors.push(format!(
                "unexpected {}:{} at {:?}",
                finding.rule_id, finding.message_id, finding.range
            ));
        }
    }
    errors
}

pub fn markdown(report: &SuiteReport) -> String {
    let mut out = String::from(
        "# Glass Lint conformance report\n\n| Case | Tool | Result | Findings |\n|---|---|---:|---:|\n",
    );
    for case in &report.cases {
        for (name, result) in &case.tools {
            out.push_str(&format!(
                "| {} | {} {} | {} | {} |\n",
                case.id,
                name,
                result.version,
                if result.passed { "pass" } else { "fail" },
                result.findings.len()
            ));
        }
    }
    for case in report
        .cases
        .iter()
        .filter(|case| case.tools.values().any(|tool| !tool.passed))
    {
        out.push_str(&format!(
            "\n## {}\n\n```js\n{}\n```\n",
            case.id, case.source
        ));
        for (tool, result) in &case.tools {
            for error in &result.errors {
                out.push_str(&format!("- `{tool}`: {error}\n"));
            }
        }
    }
    out
}

pub fn report_json(report: &LintReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn finding() -> Finding {
        Finding {
            rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
            message_id: "m".into(),
            message: "text".into(),
            severity: Severity::Warning,
            range: glass_lint_core::SourceRange {
                start: glass_lint_core::Position { line: 2, column: 3 },
                end: glass_lint_core::Position { line: 2, column: 4 },
            },
            evidence: vec![],
        }
    }
    #[test]
    fn finds_missing_diagnostic() {
        let expected = ToolExpectation {
            rules: vec![],
            required: vec![DiagnosticExpectation {
                rule_id: "test:a.b".into(),
                message_id: None,
                severity: None,
                count: 2,
                line: None,
                column: None,
                message: None,
            }],
            forbidden: vec![],
        };
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }
    #[test]
    fn flags_unexpected_diagnostic() {
        let expected = ToolExpectation {
            rules: vec![],
            required: vec![],
            forbidden: vec![],
        };
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }
}
