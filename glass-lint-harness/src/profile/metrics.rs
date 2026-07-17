//! Common report metrics and deterministic correctness digests.

use std::time::Duration;

use glass_lint_core::AnalysisReport;
use sha2::{Digest, Sha256};

use super::{ProfileFileSummary, ProfileOperationCounts, ProfileRepetitionSummary};

pub(super) fn all_diagnostic_count(report: &AnalysisReport) -> usize {
    report.diagnostics.len()
        + report
            .files
            .iter()
            .map(|file| file.diagnostics.len())
            .sum::<usize>()
}

pub(super) fn report_operation_counts(report: &AnalysisReport) -> ProfileOperationCounts {
    ProfileOperationCounts {
        files: report.operations.files,
        requests: report.operations.requests,
        edges: report.operations.edges,
        exports: report.operations.exports,
        scc_rounds: report.operations.scc_rounds,
        effect_projections: report.operations.effect_projections,
        evidence: report.operations.evidence,
    }
}

pub(super) fn evidence_order_digest(report: &AnalysisReport) -> String {
    let encoded = serde_json::to_vec(&report.files).expect("report DTOs serialize");
    format!("{:x}", Sha256::digest(encoded))
}

pub(super) fn combined_digest(digests: &[String]) -> String {
    let mut hasher = Sha256::new();
    for digest in digests {
        hasher.update(digest.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
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
    files: &[ProfileFileSummary],
) -> ProfileRepetitionSummary {
    let mut completion = glass_lint_core::ReportCompletion::Complete;
    let mut operation_counts = ProfileOperationCounts::default();
    let mut digests = Vec::new();
    let mut run_completions = Vec::new();
    for file in files {
        if file.completion == glass_lint_core::ReportCompletion::Partial {
            completion = glass_lint_core::ReportCompletion::Partial;
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
