//! Stable text and JSON renderers for harness suite results.
//!
//! Renderers consume the already-normalized report and do not reorder or
//! reinterpret findings, preserving comparisons across front ends.

#![allow(clippy::format_push_string)]

use anyhow::Result;
use glass_lint_core::AnalysisReport;

use crate::types::{CaseResult, SuiteReport, ToolResult};

#[allow(dead_code)]
fn active_tool_runs(
    report: &SuiteReport,
) -> impl Iterator<Item = (&CaseResult, &str, &ToolResult)> {
    report
        .cases
        .iter()
        .flat_map(|case| case.adapters.iter().map(move |(name, result)| (case, name.as_str(), result)))
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
            out.push_str(&format!(
                "\n{} [{}] failed with {} lint error(s), {} operational error(s), and {} finding(s)\n",
                case.id,
                tool,
                result.mismatches.len(),
                result.operational_errors.len(),
                result.findings.len()
            ));
            for mismatch in &result.mismatches {
                out.push_str(&format!("  finding mismatch: {mismatch}\n"));
            }
            for error in &result.operational_errors {
                out.push_str(&format!("  operational error: {error}\n"));
            }
            for finding in &result.findings {
                out.push_str(&format!(
                    "  finding: {}:{} at {}:{} - {}\n",
                    finding.rule_id,
                    finding.message_id,
                    finding.location.range.start().line(),
                    finding.location.range.start().column(),
                    finding.message
                ));
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
        .filter(|case| case.adapters.values().any(|tool| !tool.passed))
    {
        out.push_str(&format!(
            "\n## {}\n\n```js\n{}\n```\n",
            case.id, case.source
        ));
        for (tool, result) in &case.adapters {
            if let Some(reason) = &result.skip_reason {
                out.push_str(&format!("- `{tool}` skipped: {reason}\n"));
            }
            for error in &result.mismatches {
                out.push_str(&format!("- `{tool}` lint mismatch: {error}\n"));
            }
            for error in &result.operational_errors {
                out.push_str(&format!("- `{tool}` operational error: {error}\n"));
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

    out.push_str(&format!(
        "| Case | {} |\n|---|{}|\n",
        tool_names.join(" | "),
        tool_names
            .iter()
            .map(|_| "---:")
            .collect::<Vec<_>>()
            .join("|")
    ));

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
        out.push_str(&format!("| {} | {} |\n", case.id, counts.join(" | ")));
    }

    for case in &report.cases {
        let has_details = case
            .adapters
            .values()
            .any(|r| !r.skipped && (!r.findings.is_empty() || !r.operational_errors.is_empty()));
        if !has_details {
            continue;
        }
        out.push_str(&format!(
            "\n## {}\n\n{}\n\n```js\n{}\n```\n",
            case.id, case.description, case.source
        ));
        for (tool, result) in &case.adapters {
            if result.skipped {
                out.push_str(&format!("\n### {tool} (skipped)\n"));
                if let Some(reason) = &result.skip_reason {
                    out.push_str(&format!("\n{reason}\n"));
                }
                continue;
            }
            out.push_str(&format!(
                "\n### {tool} ({} finding(s), {} operational error(s))\n\n",
                result.findings.len(),
                result.operational_errors.len()
            ));
            for error in &result.operational_errors {
                out.push_str(&format!("- Operational error: {error}\n"));
            }
            if !result.operational_errors.is_empty() {
                out.push('\n');
            }
            if result.findings.is_empty() {
                out.push_str("No findings.\n");
            }
            for finding in &result.findings {
                out.push_str(&format!(
                    "- {}:{} at {}:{} - {}\n",
                    finding.rule_id,
                    finding.message_id,
                    finding.location.range.start().line(),
                    finding.location.range.start().column(),
                    finding.message
                ));
            }
        }
    }
    out
}

pub fn serialize_analysis_report(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
