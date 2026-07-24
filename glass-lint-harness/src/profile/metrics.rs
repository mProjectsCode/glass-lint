//! Common report metrics and deterministic correctness digests.

use std::time::Duration;

use glass_lint_core::project::AnalysisReport;
use sha2::{Digest, Sha256};

use crate::{ProfileOperationCounts, ProfileRepetitionSummary, ProfileWorkloadSummary};

pub(super) fn accumulate_report(
    report: &AnalysisReport,
    findings: &mut usize,
    diagnostics: &mut usize,
    operation_counts: &mut ProfileOperationCounts,
    evidence_digests: &mut Vec<String>,
) {
    *findings += report
        .files()
        .iter()
        .map(|file| file.findings().len())
        .sum::<usize>();
    *diagnostics += all_diagnostic_count(report);
    *operation_counts += report_operation_counts(report);
    evidence_digests.push(evidence_order_digest(report));
}

pub(super) fn all_diagnostic_count(report: &AnalysisReport) -> usize {
    report.diagnostics().len()
        + report
            .files()
            .iter()
            .map(|file| file.diagnostics().len())
            .sum::<usize>()
}

pub(super) fn report_operation_counts(report: &AnalysisReport) -> ProfileOperationCounts {
    report.operations()
}

pub(super) fn evidence_order_digest(report: &AnalysisReport) -> String {
    let encoded = serde_json::to_vec(report.files()).expect("report DTOs serialize");
    format!("{:?}", Sha256::digest(encoded))
}

pub(super) fn combined_digest(digests: &[String]) -> String {
    let mut hasher = Sha256::new();
    for digest in digests {
        hasher.update(digest.as_bytes());
        hasher.update([0]);
    }
    format!("{:?}", hasher.finalize())
}

pub(super) fn median_duration(repetitions: &[ProfileRepetitionSummary]) -> Duration {
    let mut durations = repetitions
        .iter()
        .map(|repetition| repetition.duration)
        .collect::<Vec<_>>();
    durations.sort_unstable();
    durations
        .get(durations.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

pub(super) fn repetition_from_files(
    duration: Duration,
    files: &[ProfileWorkloadSummary],
) -> ProfileRepetitionSummary {
    let mut completion = glass_lint_core::project::ReportCompletion::Complete;
    let mut operation_counts = ProfileOperationCounts::default();
    let mut digests = Vec::new();
    let mut run_completions = Vec::new();
    for file in files {
        if file.completion == glass_lint_core::project::ReportCompletion::Partial {
            completion = glass_lint_core::project::ReportCompletion::Partial;
        }
        operation_counts += file.operation_counts;
        digests.push(file.evidence_order_digest.clone());
        run_completions.extend(file.run_completions.iter().copied());
    }
    ProfileRepetitionSummary {
        duration,
        findings: files.iter().map(|file| file.findings).sum(),
        diagnostics: files.iter().map(|file| file.diagnostics).sum(),
        completion,
        run_completions,
        operation_counts,
        evidence_order_digest: combined_digest(&digests),
    }
}
