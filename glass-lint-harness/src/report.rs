use anyhow::Result;
use glass_lint_core::LintReport;

use crate::types::SuiteReport;

pub fn summary(report: &SuiteReport) -> String {
    let cases = report.cases.len();
    let tool_runs = report
        .cases
        .iter()
        .flat_map(|case| case.tools.values())
        .filter(|tool| !tool.skipped)
        .count();
    let skipped = report
        .cases
        .iter()
        .flat_map(|case| case.tools.values())
        .filter(|tool| tool.skipped)
        .count();
    let failed = report
        .cases
        .iter()
        .flat_map(|case| case.tools.values())
        .filter(|tool| !tool.passed)
        .count();
    let findings = report
        .cases
        .iter()
        .flat_map(|case| case.tools.values())
        .map(|tool| tool.findings.len())
        .sum::<usize>();
    let passed = tool_runs.saturating_sub(failed);

    format!(
        "Harness: {cases} case(s), {tool_runs} run(s), {passed} passed, {failed} failed, {skipped} skipped, {findings} finding(s)"
    )
}

pub fn failure_details(report: &SuiteReport) -> String {
    let mut out = String::new();
    for case in &report.cases {
        for (tool, result) in &case.tools {
            if result.passed {
                continue;
            }
            out.push_str(&format!(
                "\n{} [{}] failed with {} error(s) and {} finding(s)\n",
                case.id,
                tool,
                result.errors.len(),
                result.findings.len()
            ));
            for error in &result.errors {
                out.push_str(&format!("  error: {error}\n"));
            }
            for finding in &result.findings {
                out.push_str(&format!(
                    "  finding: {}:{} at {}:{} - {}\n",
                    finding.rule_id,
                    finding.message_id,
                    finding.range.start.line,
                    finding.range.start.column,
                    finding.message
                ));
            }
        }
    }
    out
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
