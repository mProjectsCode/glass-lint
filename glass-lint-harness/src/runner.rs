//! Case execution and expectation comparison.
//!
//! The runner records one result per case/tool, treating skipped tools as
//! explicit successful non-runs and preserving adapter timing by name.

use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::Result;
use glass_lint_core::project::Finding;
use tracing::info;

use crate::{
    adapters::{Adapter, lint_generated_source},
    bundler::{Bundler, MAX_GENERATED_BYTES, ProcessBundler, request_for_case},
    cases::load_cases,
    types::{
        BundleKey, BundleResult, BundleTarget, BundleTransformer, CaseResult, ExpectedCount,
        FindingExpectation, SuiteReport, ToolExpectation, ToolResult,
    },
};

pub type AdapterTimings = BTreeMap<String, Duration>;
pub type BundleTimings = BTreeMap<BundleKey, Duration>;

/// Execute every configured adapter against every discovered case.
pub fn run_suite(
    root: &Path,
    adapters: &[Box<dyn Adapter>],
) -> Result<(SuiteReport, Vec<AdapterTimings>)> {
    let bundler = ProcessBundler::default();
    let (report, timings, _) = run_suite_with_bundler(root, adapters, &bundler)?;
    Ok((report, timings))
}

pub fn run_suite_with_bundler(
    root: &Path,
    adapters: &[Box<dyn Adapter>],
    bundler: &dyn Bundler,
) -> Result<(SuiteReport, Vec<AdapterTimings>, Vec<BundleTimings>)> {
    let cases = load_cases(root)?;
    let mut results = Vec::new();
    let mut all_timings = Vec::new();
    let mut all_bundle_timings = Vec::new();
    for case in &cases {
        let mut tools = BTreeMap::new();
        let mut timings = BTreeMap::new();
        for adapter in adapters {
            let tool_start = Instant::now();
            let version = adapter
                .version()
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let Some(expectation) = case.adapters.get(adapter.name()) else {
                timings.insert(adapter.name().into(), tool_start.elapsed());
                tools.insert(
                    adapter.name().into(),
                    ToolResult::skipped(version, Some("tool not configured for this case".into())),
                );
                continue;
            };
            if case.project.is_some() && !adapter.supports_projects() {
                let reason = "adapter does not support project-shaped requests".to_string();
                timings.insert(adapter.name().into(), tool_start.elapsed());
                tools.insert(
                    adapter.name().into(),
                    ToolResult::skipped(version, Some(reason)),
                );
                continue;
            }
            let (findings, errors, operational_errors) =
                match adapter.run_with_locations(case, expectation) {
                    Ok(output) => {
                        let errors = compare(&output.findings, expectation);
                        (output.findings, errors, vec![])
                    }
                    Err(error) => (vec![], vec![], vec![error.to_string()]),
                };
            timings.insert(adapter.name().into(), tool_start.elapsed());
            tools.insert(
                adapter.name().into(),
                ToolResult {
                    version,
                    skipped: false,
                    skip_reason: None,
                    passed: errors.is_empty() && operational_errors.is_empty(),
                    findings,
                    mismatches: errors,
                    operational_errors,
                },
            );
        }
        let (bundle_results, bundle_timings) = run_bundles(case, &tools, bundler);
        let total: Duration = timings.values().sum();
        let details = timings
            .iter()
            .map(|(name, dur)| format!("{name} {dur:.1?}"))
            .collect::<Vec<_>>()
            .join(", ");
        info!(progress = format!("  {}: {total:.1?} ({})", case.id, details));
        all_timings.push(timings);
        results.push(CaseResult {
            id: case.id.clone(),
            description: case.description.clone(),
            source: case.source.clone(),
            adapters: tools,
            bundles: bundle_results,
        });
        all_bundle_timings.push(bundle_timings);
    }
    Ok((
        SuiteReport {
            schema_version: 2,
            cases: results,
        },
        all_timings,
        all_bundle_timings,
    ))
}

fn run_bundles(
    case: &crate::types::Case,
    tools: &BTreeMap<String, ToolResult>,
    bundler: &dyn Bundler,
) -> (Vec<BundleResult>, BundleTimings) {
    if case.bundles().is_empty() {
        return (Vec::new(), BTreeMap::new());
    }
    let Some(expectation) = case.adapters.get("glass-lint") else {
        return (
            vec![BundleResult {
                key: BundleKey {
                    profile: case.bundles()[0],
                    transformer: BundleTransformer::Vite,
                    minified: false,
                    target: BundleTarget::Es5,
                },
                transformer_version: None,
                passed: false,
                authored_counts: BTreeMap::new(),
                transformed_counts: BTreeMap::new(),
                mismatches: Vec::new(),
                operational_errors: vec!["bundled case has no glass-lint expectation".into()],
                generated_source_bytes: 0,
                generated_source_digest: None,
                generated_source: None,
            }],
            BTreeMap::new(),
        );
    };
    let Some(authored) = tools.get("glass-lint") else {
        return (
            vec![bundle_operational_result(
                &BundleKey {
                    profile: case.bundles()[0],
                    transformer: BundleTransformer::Vite,
                    minified: false,
                    target: BundleTarget::Es5,
                },
                "glass-lint adapter was not configured for this run",
            )],
            BTreeMap::new(),
        );
    };
    if authored.skipped || !authored.operational_errors.is_empty() {
        return (Vec::new(), BTreeMap::new());
    }
    let authored_counts = rule_counts(&authored.findings);
    let mut results = Vec::new();
    let mut timings = BTreeMap::new();
    for &profile in case.bundles() {
        for transformer in BundleTransformer::all() {
            for minified in [false, true] {
                for target in BundleTarget::all() {
                    let key = BundleKey {
                        profile,
                        transformer,
                        minified,
                        target,
                    };
                    let start = Instant::now();
                    let result =
                        execute_bundle(case, expectation, &authored_counts, key.clone(), bundler);
                    timings.insert(key, start.elapsed());
                    results.push(result);
                }
            }
        }
    }
    (results, timings)
}

fn execute_bundle(
    case: &crate::types::Case,
    expectation: &ToolExpectation,
    authored_counts: &BTreeMap<String, usize>,
    key: BundleKey,
    bundler: &dyn Bundler,
) -> BundleResult {
    let request = request_for_case(case, key.profile, key.transformer, key.minified, key.target);
    let output = match bundler.bundle(&request) {
        Ok(output) => output,
        Err(error) => {
            return bundle_operational_result_with_counts(&key, authored_counts, error.to_string());
        }
    };
    let transformed = match lint_generated_source(&request.entry, &output.source, expectation) {
        Ok(findings) => findings,
        Err(error) => {
            return BundleResult {
                key,
                transformer_version: Some(output.transformer_version),
                passed: false,
                authored_counts: authored_counts.clone(),
                transformed_counts: BTreeMap::new(),
                mismatches: Vec::new(),
                operational_errors: vec![format!("generated source analysis failed: {error}")],
                generated_source_bytes: output.bytes,
                generated_source_digest: Some(output.digest),
                generated_source: Some(output.source),
            };
        }
    };
    let transformed_counts = rule_counts(&transformed);
    let mismatches = compare_rule_counts(authored_counts, &transformed_counts, &key);
    let passed = mismatches.is_empty();
    BundleResult {
        key,
        transformer_version: Some(output.transformer_version),
        passed,
        authored_counts: authored_counts.clone(),
        transformed_counts,
        mismatches,
        operational_errors: Vec::new(),
        generated_source_bytes: output.bytes,
        generated_source_digest: Some(output.digest),
        generated_source: if passed || output.source.len() > MAX_GENERATED_BYTES {
            None
        } else {
            Some(output.source)
        },
    }
}

fn bundle_operational_result(key: &BundleKey, error: impl Into<String>) -> BundleResult {
    bundle_operational_result_with_counts(key, &BTreeMap::new(), error)
}

fn bundle_operational_result_with_counts(
    key: &BundleKey,
    authored_counts: &BTreeMap<String, usize>,
    error: impl Into<String>,
) -> BundleResult {
    BundleResult {
        key: key.clone(),
        transformer_version: None,
        passed: false,
        authored_counts: authored_counts.clone(),
        transformed_counts: BTreeMap::new(),
        mismatches: Vec::new(),
        operational_errors: vec![error.into()],
        generated_source_bytes: 0,
        generated_source_digest: None,
        generated_source: None,
    }
}

fn rule_counts(findings: &[Finding]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        *counts.entry(finding.rule_id().to_string()).or_default() += 1;
    }
    counts
}

pub fn compare_rule_counts(
    authored: &BTreeMap<String, usize>,
    transformed: &BTreeMap<String, usize>,
    key: &BundleKey,
) -> Vec<String> {
    authored
        .keys()
        .chain(transformed.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|rule| {
            let before = authored.get(rule).copied().unwrap_or_default();
            let after = transformed.get(rule).copied().unwrap_or_default();
            (before != after).then(|| {
                format!(
                    "{} rule {}: before={}, after={}",
                    key.label(),
                    rule,
                    before,
                    after
                )
            })
        })
        .collect()
}

impl FindingExpectation {
    fn matches(&self, finding: &Finding) -> bool {
        *finding.rule_id() == self.rule_id
            && self
                .severity
                .is_none_or(|severity| finding.severity() == severity)
            && self
                .line
                .is_none_or(|line| finding.location().range().start().line() == line)
            && self
                .column
                .is_none_or(|column| finding.location().range().start().column() == column)
            && self
                .message
                .as_ref()
                .is_none_or(|message| finding.message() == message.as_str())
            && self
                .path
                .as_ref()
                .is_none_or(|path| finding.location().path() == path)
            && self
                .certainty
                .is_none_or(|certainty| finding.certainty() == certainty)
    }
}

fn compare(findings: &[Finding], expectation: &ToolExpectation) -> Vec<String> {
    let mut errors = Vec::new();
    for expected in expectation.required() {
        let actual = matching_count(findings, expected);
        let count_matches = match expected.count {
            ExpectedCount::Exactly(count) => actual == count,
            ExpectedCount::AtLeastOne => actual > 0,
        };
        if !count_matches {
            errors.push(format!(
                "expected {:?} x {}, found {}",
                expected.count, expected.rule_id, actual
            ));
        }
    }
    for forbidden in expectation.forbidden() {
        let actual = matching_count(findings, forbidden);
        if actual > 0 {
            errors.push(format!(
                "forbidden diagnostic {} appeared {} time(s)",
                forbidden.rule_id, actual
            ));
        }
    }
    for finding in findings {
        let is_required = expectation
            .required()
            .iter()
            .any(|expected| expected.matches(finding));
        let is_forbidden = expectation
            .forbidden()
            .iter()
            .any(|forbidden| forbidden.matches(finding));
        if !is_required && !is_forbidden {
            errors.push(format!(
                "unexpected {} at {:?}",
                finding.rule_id(),
                finding.location().range()
            ));
        }
    }
    errors
}

fn matching_count(findings: &[Finding], expected: &FindingExpectation) -> usize {
    findings
        .iter()
        .filter(|finding| expected.matches(finding))
        .count()
}

#[cfg(test)]
mod tests;
