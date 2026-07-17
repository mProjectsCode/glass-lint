//! Stable text and JSON renderers for harness suite results.
//!
//! Renderers consume the already-normalized report and do not reorder or
//! reinterpret findings, preserving comparisons across front ends.

#![allow(clippy::format_push_string)]

use anyhow::Result;
use glass_lint_core::AnalysisReport;

use crate::types::SuiteReport;

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn comparison(report: &SuiteReport) -> String {
    let tool_names: Vec<String> = report
        .cases
        .iter()
        .flat_map(|case| case.tools.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect();

    let mut out = String::from(
        "# Glass Lint and ESLint comparison\n\n\
         This generated report compares findings on the end-to-end fixture suite.\n\
         The tools have different rule catalogs, so counts are descriptive rather\n\
         than precision or recall scores. Run `make compare` to regenerate it.\n\n",
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
                case.tools.get(name).map_or_else(
                    || "-".into(),
                    |r| {
                        if r.skipped {
                            "skip".into()
                        } else {
                            r.findings.len().to_string()
                        }
                    },
                )
            })
            .collect();
        out.push_str(&format!("| {} | {} |\n", case.id, counts.join(" | ")));
    }

    for case in &report.cases {
        let has_findings = case
            .tools
            .values()
            .any(|r| !r.skipped && !r.findings.is_empty());
        if !has_findings {
            continue;
        }
        out.push_str(&format!(
            "\n## {}\n\n{}\n\n```js\n{}\n```\n",
            case.id, case.description, case.source
        ));
        for (tool, result) in &case.tools {
            if result.skipped {
                out.push_str(&format!("\n### {tool} (skipped)\n"));
                if let Some(reason) = &result.skip_reason {
                    out.push_str(&format!("\n{reason}\n"));
                }
                continue;
            }
            out.push_str(&format!(
                "\n### {tool} ({} finding(s))\n\n",
                result.findings.len()
            ));
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

pub fn report_json(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}
