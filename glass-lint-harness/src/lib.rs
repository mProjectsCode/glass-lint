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

pub fn load_cases(root: &Path) -> Result<Vec<Case>> {
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "js")
        })
        .map(|entry| entry.into_path())
        .collect();
    paths.sort();
    let mut ids = BTreeSet::new();
    paths
        .into_iter()
        .map(|path| {
            let source =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let case = parse_case(root, &path, source)
                .with_context(|| format!("parse {}", path.display()))?;
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

fn parse_case(root: &Path, path: &Path, source: String) -> Result<Case> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let id = relative
        .with_extension("")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(default_filename);
    let mut case = Case {
        id: id.clone(),
        description: id,
        tags: vec![],
        language: default_language(),
        filename,
        source,
        tools: BTreeMap::new(),
    };

    let lines: Vec<_> = case.source.lines().map(str::to_owned).collect();
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let Some(comment) = trimmed.strip_prefix("//") else {
            break;
        };
        let directive = comment.trim();
        if let Some(rest) = directive.strip_prefix("@case ") {
            parse_case_directive(&mut case, rest)?;
        } else if let Some(rest) = directive.strip_prefix("@tool ") {
            parse_tool_directive(&mut case, rest)?;
        }
    }

    for (index, line) in lines.iter().enumerate() {
        let Some(comment_start) = line.find("// @") else {
            continue;
        };
        let directive = line[comment_start + 3..].trim();
        if let Some(rest) = directive.strip_prefix("@expect-error-after ") {
            let line_number = previous_code_line(&lines, index)
                .with_context(|| format!("{}:{} has no previous code line", case.id, index + 1))?;
            add_expectation(&mut case, rest, line_number)?;
        } else if let Some(rest) = directive.strip_prefix("@expect-error ") {
            let line_number = if line[..comment_start].trim().is_empty() {
                (index + 2) as u32
            } else {
                (index + 1) as u32
            };
            add_expectation(&mut case, rest, line_number)?;
        }
    }

    case.source = strip_harness_comments(&case.source);
    Ok(case)
}

fn strip_harness_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let Some(comment_start) = line.find("// @") else {
                return line.to_owned();
            };
            let directive = line[comment_start + 3..].trim();
            if directive.starts_with("@case ")
                || directive.starts_with("@tool ")
                || directive.starts_with("@expect-error ")
                || directive.starts_with("@expect-error-after ")
            {
                format!(
                    "{}{}",
                    &line[..comment_start],
                    " ".repeat(line.len() - comment_start)
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn previous_code_line(lines: &[String], assertion_index: usize) -> Option<u32> {
    lines[..assertion_index]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("@expect-error")
                && !trimmed.starts_with("@expect-error-after")
        })
        .map(|(index, _)| (index + 1) as u32)
}

fn parse_case_directive(case: &mut Case, rest: &str) -> Result<()> {
    let (key, value) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @case directive `{rest}`"))?;
    match key {
        "id" => case.id = value.trim().into(),
        "description" => case.description = value.trim().into(),
        "tags" => {
            case.tags = value
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect();
        }
        "filename" => case.filename = value.trim().into(),
        "language" => case.language = value.trim().into(),
        _ => bail!("unknown @case key `{key}`"),
    }
    Ok(())
}

fn parse_tool_directive(case: &mut Case, rest: &str) -> Result<()> {
    let (name, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @tool directive `{rest}`"))?;
    let mut expectation = ToolExpectation {
        rules: vec![],
        required: vec![],
        forbidden: vec![],
    };
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "rules" => {
                expectation.rules = value
                    .split(',')
                    .map(str::trim)
                    .filter(|rule| !rule.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => bail!("unknown @tool field `{key}`"),
        }
    }
    if expectation.rules.is_empty() {
        bail!("@tool {name} must specify rules=");
    }
    case.tools.insert(name.into(), expectation);
    Ok(())
}

fn add_expectation(case: &mut Case, rest: &str, line: u32) -> Result<()> {
    let (tool, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @expect-error directive `{rest}`"))?;
    let expectation = case
        .tools
        .get_mut(tool)
        .with_context(|| format!("@expect-error references unconfigured tool `{tool}`"))?;
    let mut diagnostic = DiagnosticExpectation {
        rule_id: String::new(),
        message_id: None,
        severity: None,
        count: Some(1),
        line: Some(line),
        column: None,
        message: None,
    };
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "rule" => diagnostic.rule_id = value,
            "message_id" => diagnostic.message_id = Some(value),
            "severity" => diagnostic.severity = Some(parse_severity(&value)?),
            "count" => diagnostic.count = parse_optional_usize(&value)?,
            "line" => diagnostic.line = parse_optional_u32(&value)?,
            "column" => diagnostic.column = parse_optional_u32(&value)?,
            "message" => diagnostic.message = Some(value),
            _ => bail!("unknown @expect-error field `{key}`"),
        }
    }
    if diagnostic.rule_id.is_empty() {
        bail!("@expect-error for {tool} must specify rule=");
    }
    expectation.required.push(diagnostic);
    Ok(())
}

fn parse_fields(fields: &str) -> Result<Vec<(String, String)>> {
    fields
        .split_whitespace()
        .map(|field| {
            let (key, value) = field
                .split_once('=')
                .with_context(|| format!("expected key=value, found `{field}`"))?;
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn parse_severity(value: &str) -> Result<Severity> {
    match value {
        "info" => Ok(Severity::Info),
        "warning" => Ok(Severity::Warning),
        "error" => Ok(Severity::Error),
        _ => bail!("unknown severity `{value}`"),
    }
}

fn parse_optional_u32(value: &str) -> Result<Option<u32>> {
    if value == "any" {
        Ok(None)
    } else {
        Ok(Some(value.parse()?))
    }
}

fn parse_optional_usize(value: &str) -> Result<Option<usize>> {
    if value == "any" {
        Ok(None)
    } else {
        Ok(Some(value.parse()?))
    }
}

pub fn run_suite(root: &Path, adapters: &[Box<dyn Adapter>]) -> Result<SuiteReport> {
    let cases = load_cases(root)?;
    let mut results = Vec::new();
    for case in cases {
        let mut tools = BTreeMap::new();
        for adapter in adapters {
            let version = adapter
                .version()
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let Some(expectation) = case.tools.get(adapter.name()) else {
                tools.insert(
                    adapter.name().into(),
                    ToolResult {
                        version,
                        skipped: true,
                        skip_reason: Some("tool not configured for this case".into()),
                        passed: true,
                        findings: vec![],
                        errors: vec![],
                    },
                );
                continue;
            };
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
                    skipped: false,
                    skip_reason: None,
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
        if expected.count.is_some_and(|count| actual != count) {
            errors.push(format!(
                "expected {} × {}, found {}",
                expected.count.unwrap(),
                expected.rule_id,
                actual
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
                if result.skipped {
                    "skip"
                } else if result.passed {
                    "pass"
                } else {
                    "fail"
                },
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
            if let Some(reason) = &result.skip_reason {
                out.push_str(&format!("- `{tool}` skipped: {reason}\n"));
            }
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
                count: Some(2),
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
    #[test]
    fn parses_comment_case() {
        let source = "\
// @case description Dynamic code
// @tool glass-lint rules=obsidian:dynamic_code
// @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
globalThis.setTimeout('run()', 10);
";
        let case = parse_case(
            Path::new("tests/cases"),
            Path::new("tests/cases/system/timer.js"),
            source.into(),
        )
        .unwrap();
        assert_eq!(case.id, "system/timer");
        assert_eq!(case.description, "Dynamic code");
        assert_eq!(case.tools["glass-lint"].required[0].line, Some(4));
    }
}
