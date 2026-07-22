//! Stable text and JSON renderers for harness suite results.
//!
//! Renderers consume the already-normalized report and do not reorder or
//! reinterpret findings, preserving comparisons across front ends.

use std::fmt::Write;

use anyhow::Result;
use glass_lint_core::AnalysisReport;

use crate::types::{CaseResult, SuiteReport, ToolResult};

#[allow(dead_code)]
fn active_tool_runs(
    report: &SuiteReport,
) -> impl Iterator<Item = (&CaseResult, &str, &ToolResult)> {
    report.cases.iter().flat_map(|case| {
        case.adapters
            .iter()
            .map(move |(name, result)| (case, name.as_str(), result))
    })
}

#[must_use]
pub fn render_suite_summary(report: &SuiteReport) -> String {
    let cases = report.cases.len();
    let tool_runs = report
        .cases
        .iter()
        .flat_map(|case| case.adapters.values())
        .filter(|tool| !tool.skipped)
        .count();
    let skipped = report
        .cases
        .iter()
        .flat_map(|case| case.adapters.values())
        .filter(|tool| tool.skipped)
        .count();
    let failed = report
        .cases
        .iter()
        .flat_map(|case| case.adapters.values())
        .filter(|tool| !tool.passed)
        .count();
    let findings = report
        .cases
        .iter()
        .flat_map(|case| case.adapters.values())
        .map(|tool| tool.findings.len())
        .sum::<usize>();
    let passed = tool_runs.saturating_sub(failed);

    format!(
        "Harness: {cases} case(s), {tool_runs} run(s), {passed} passed, {failed} failed, {skipped} skipped, {findings} finding(s)"
    )
}

#[must_use]
pub fn render_suite_failures(report: &SuiteReport) -> String {
    let mut out = String::new();
    for case in &report.cases {
        for (tool, result) in &case.adapters {
            if result.passed {
                continue;
            }
            writeln!(
                out,
                "\n{} [{}] failed with {} lint error(s), {} operational error(s), and {} finding(s)",
                case.id,
                tool,
                result.mismatches.len(),
                result.operational_errors.len(),
                result.findings.len()
            )
            .expect("writing to a String cannot fail");
            for mismatch in &result.mismatches {
                let _ = writeln!(out, "  finding mismatch: {mismatch}");
            }
            for error in &result.operational_errors {
                let _ = writeln!(out, "  operational error: {error}");
            }
            for finding in &result.findings {
                let _ = writeln!(
                    out,
                    "  finding: {}:{} at {}:{} - {}",
                    finding.rule_id,
                    finding.message_id,
                    finding.location.range.start().line(),
                    finding.location.range.start().column(),
                    finding.message
                );
            }
        }
    }
    out
}

#[must_use]
pub fn render_suite_markdown(report: &SuiteReport) -> String {
    let mut out = String::from(
        "# Glass Lint conformance report\n\n| Case | Tool | Result | Findings |\n|---|---|---:|---:|\n",
    );
    for case in &report.cases {
        for (name, result) in &case.adapters {
            let _ = writeln!(
                out,
                "| {} | {} {} | {} | {} |",
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
            );
        }
    }
    for case in report
        .cases
        .iter()
        .filter(|case| case.adapters.values().any(|tool| !tool.passed))
    {
        let _ = writeln!(out, "\n## {}\n\n```js\n{}\n```", case.id, case.source);
        for (tool, result) in &case.adapters {
            if let Some(reason) = &result.skip_reason {
                let _ = writeln!(out, "- `{tool}` skipped: {reason}");
            }
            for error in &result.mismatches {
                let _ = writeln!(out, "- `{tool}` lint mismatch: {error}");
            }
            for error in &result.operational_errors {
                let _ = writeln!(out, "- `{tool}` operational error: {error}");
            }
        }
    }
    out
}

#[must_use]
pub fn render_adapter_comparison(report: &SuiteReport) -> String {
    let tool_names: Vec<String> = report
        .cases
        .iter()
        .flat_map(|case| case.adapters.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect();

    let mut out = String::from(
        "# Adapter comparison\n\n\
         This generated report compares findings from the configured adapters\n\
         on the loaded suite. Adapter catalogs differ, so counts are descriptive\n\
         rather than precision or recall scores.\n\n",
    );

    let _ = writeln!(
        out,
        "| Case | {} |\n|---|{}|",
        tool_names.join(" | "),
        tool_names
            .iter()
            .map(|_| "---:")
            .collect::<Vec<_>>()
            .join("|")
    );

    for case in &report.cases {
        let counts: Vec<String> = tool_names
            .iter()
            .map(|name| {
                case.adapters.get(name).map_or_else(
                    || "-".into(),
                    |r| {
                        if r.skipped {
                            "skip".into()
                        } else if r.operational_errors.is_empty() {
                            r.findings.len().to_string()
                        } else {
                            format!(
                                "{} ({} operational error(s))",
                                r.findings.len(),
                                r.operational_errors.len()
                            )
                        }
                    },
                )
            })
            .collect();
        let _ = writeln!(out, "| {} | {} |", case.id, counts.join(" | "));
    }

    for case in &report.cases {
        let has_details = case
            .adapters
            .values()
            .any(|r| !r.skipped && (!r.findings.is_empty() || !r.operational_errors.is_empty()));
        if !has_details {
            continue;
        }
        let _ = writeln!(
            out,
            "\n## {}\n\n{}\n\n```js\n{}\n```",
            case.id, case.description, case.source
        );
        for (tool, result) in &case.adapters {
            if result.skipped {
                let _ = writeln!(out, "\n### {tool} (skipped)");
                if let Some(reason) = &result.skip_reason {
                    let _ = writeln!(out, "\n{reason}");
                }
                continue;
            }
            let _ = writeln!(
                out,
                "\n### {tool} ({} finding(s), {} operational error(s))\n",
                result.findings.len(),
                result.operational_errors.len()
            );
            for error in &result.operational_errors {
                let _ = writeln!(out, "- Operational error: {error}");
            }
            if !result.operational_errors.is_empty() {
                out.push('\n');
            }
            if result.findings.is_empty() {
                out.push_str("No findings.\n");
            }
            for finding in &result.findings {
                let _ = writeln!(
                    out,
                    "- {}:{} at {}:{} - {}",
                    finding.rule_id,
                    finding.message_id,
                    finding.location.range.start().line(),
                    finding.location.range.start().column(),
                    finding.message
                );
            }
        }
    }
    out
}

pub fn serialize_analysis_report(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
