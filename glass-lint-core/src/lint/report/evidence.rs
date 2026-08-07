use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::SourceRange;

use crate::{
    analysis::{ProjectSemanticModel, display_span},
    api::classification::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, ClassificationResult,
        MatchedCapability, RuleIndex,
    },
    diagnostic::SourceLineIndex,
    lint::report::{ProjectReportSession, ReportAssembly},
    project::{
        EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, FileReport, Finding,
        MatchCertainty, ModuleId, ProjectRelativePath, SourceLocation,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EvidenceOccurrenceRef {
    evidence: usize,
    occurrence: usize,
}

impl EvidenceOccurrenceRef {
    fn resolve(
        self,
        evidence_items: &[ClassificationEvidence],
    ) -> Option<(&ClassificationEvidence, &ClassificationEvidenceOccurrence)> {
        let evidence = evidence_items.get(self.evidence)?;
        Some((evidence, evidence.occurrences().get(self.occurrence)?))
    }
}

#[derive(Debug)]
struct EvidenceRangeEntry {
    range: SourceRange,
    occurrences: Vec<EvidenceOccurrenceRef>,
}

#[derive(Debug)]
struct FindingGroup {
    range: SourceRange,
    occurrences: Vec<EvidenceOccurrenceRef>,
}

impl FindingGroup {
    fn new(range: SourceRange) -> Self {
        Self {
            range,
            occurrences: Vec::new(),
        }
    }

    fn add_entry(&mut self, entry: &EvidenceRangeEntry) {
        if self.range.contains(&entry.range) {
            self.occurrences.extend(entry.occurrences.iter().copied());
        }
    }

    fn is_truncated(&self, evidence_items: &[ClassificationEvidence]) -> bool {
        self.occurrences
            .iter()
            .filter_map(|reference| reference.resolve(evidence_items))
            .any(|(evidence, _)| evidence.is_truncated())
    }

    fn certainty(&self, evidence_items: &[ClassificationEvidence]) -> MatchCertainty {
        if self
            .occurrences
            .iter()
            .filter_map(|reference| reference.resolve(evidence_items))
            .any(|(evidence, _)| evidence.certainty() == MatchCertainty::Definite)
        {
            MatchCertainty::Definite
        } else {
            MatchCertainty::Possible
        }
    }
}

pub(super) fn populate_project_files(
    assembly: &ReportAssembly<'_>,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    classifications: &BTreeMap<ModuleId, ClassificationResult>,
    files: &mut BTreeMap<ProjectRelativePath, FileReport>,
) {
    for module in project.modules() {
        let Some(classification) = classifications.get(&module.id()) else {
            continue;
        };
        let findings = findings_for_module(assembly, project, session, module, classification);
        let mut findings = merge_duplicate_findings(findings);
        findings.sort_by(compare_findings);
        files.insert(
            module.path().clone(),
            FileReport::new(module.path().clone(), findings, Vec::new()),
        );
    }
}

fn compare_findings(left: &Finding, right: &Finding) -> std::cmp::Ordering {
    let left_range = left.location().range();
    let right_range = right.location().range();
    (
        left_range.start().line(),
        left_range.start().column(),
        left_range.end().line(),
        left_range.end().column(),
        left.rule_id().as_str(),
        left.message(),
        left.severity(),
    )
        .cmp(&(
            right_range.start().line(),
            right_range.start().column(),
            right_range.end().line(),
            right_range.end().column(),
            right.rule_id().as_str(),
            right.message(),
            right.severity(),
        ))
}

fn merge_duplicate_findings(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by(compare_findings);
    let mut merged: Vec<Finding> = Vec::with_capacity(findings.len());
    for finding in findings {
        match merged.pop() {
            Some(previous) if previous.has_primary(&finding) => {
                merged.push(previous.merge_duplicate(&finding));
            }
            Some(previous) => {
                merged.push(previous);
                merged.push(finding);
            }
            None => merged.push(finding),
        }
    }
    merged
}

fn findings_for_module(
    assembly: &ReportAssembly<'_>,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    module: &crate::analysis::ProjectModule,
    classification: &ClassificationResult,
) -> Vec<Finding> {
    let lines = module.source_context().lines();
    let path = module.path();
    let mut rule_findings: BTreeMap<RuleIndex, Vec<Finding>> = BTreeMap::new();
    for capability in classification.capabilities() {
        rule_findings
            .entry(capability.rule_index())
            .or_default()
            .extend(findings_for_capability(
                assembly, project, session, capability, lines, path,
            ));
    }
    rule_findings.into_values().flatten().collect()
}

fn range_entries(
    evidence_items: &[ClassificationEvidence],
    lines: &SourceLineIndex,
) -> Vec<EvidenceRangeEntry> {
    let mut by_range: BTreeMap<SourceRange, Vec<EvidenceOccurrenceRef>> = BTreeMap::new();
    for (evidence_idx, evidence) in evidence_items.iter().enumerate() {
        for (occurrence_idx, occurrence) in evidence.occurrences().iter().enumerate() {
            let span = display_span(lines, occurrence.span(), evidence.kind(), evidence.symbol());
            if span.is_empty() {
                continue;
            }
            let Ok(range) = lines.try_range(span) else {
                continue;
            };
            by_range
                .entry(range)
                .or_default()
                .push(EvidenceOccurrenceRef {
                    evidence: evidence_idx,
                    occurrence: occurrence_idx,
                });
        }
    }
    by_range
        .into_iter()
        .map(|(range, occurrences)| EvidenceRangeEntry { range, occurrences })
        .collect()
}

fn finding_groups(entries: &[EvidenceRangeEntry]) -> Vec<FindingGroup> {
    let mut ranges: Vec<SourceRange> = entries.iter().map(|entry| entry.range.clone()).collect();
    crate::lint::ranges::remove_contained_ranges(&mut ranges);
    let mut groups = Vec::with_capacity(ranges.len());
    let mut entry_cursor = 0usize;
    for retained in ranges {
        while entry_cursor < entries.len() && entries[entry_cursor].range.end() < retained.start() {
            entry_cursor += 1;
        }
        let mut group = FindingGroup::new(retained);
        let mut scan = entry_cursor;
        while scan < entries.len() && entries[scan].range.start() <= group.range.end() {
            group.add_entry(&entries[scan]);
            scan += 1;
        }
        groups.push(group);
    }
    groups
}

fn findings_for_capability(
    assembly: &ReportAssembly<'_>,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    capability: &MatchedCapability,
    lines: &SourceLineIndex,
    path: &ProjectRelativePath,
) -> Vec<Finding> {
    let Some(rule_id) = assembly.catalog.rule_id(capability.rule_index()).cloned() else {
        return Vec::new();
    };
    let evidence_items = capability.evidence();
    if evidence_items.is_empty() {
        return Vec::new();
    }
    let entries = range_entries(evidence_items, lines);
    let groups = finding_groups(&entries);
    groups
        .into_iter()
        .filter_map(|group| {
            let mut traces = BTreeSet::new();
            for reference in &group.occurrences {
                let Some((ev, occurrence)) = reference.resolve(evidence_items) else {
                    continue;
                };
                let steps = occurrence.trace().map_or_else(
                    || Some(fallback_trace(ev, path, &group.range)),
                    |trace_id| resolve_trace(trace_id, project, session),
                );
                if let Some(steps) = steps
                    && !steps.is_empty()
                    && let Ok(trace) = EvidenceTrace::new(steps)
                {
                    traces.insert(trace);
                }
            }
            if traces.is_empty()
                && let Ok(trace) = EvidenceTrace::new(vec![EvidenceStep::new(
                    EvidenceRole::Occurrence,
                    "evidence occurrence".into(),
                    SourceLocation::new(path.clone(), group.range.clone()),
                )])
            {
                traces.insert(trace);
            }
            let evidence = EvidenceTraces::with_truncation(
                traces.into_iter().collect(),
                group.is_truncated(evidence_items),
            )
            .ok()?;
            Finding::new(
                rule_id.clone(),
                capability.label().to_string(),
                capability.severity(),
                SourceLocation::new(path.clone(), group.range.clone()),
                evidence,
                group.certainty(evidence_items),
            )
            .into()
        })
        .collect()
}

fn resolve_trace(
    head: crate::analysis::trace::TraceNodeId,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
) -> Option<Vec<EvidenceStep>> {
    let raw = session.reconstruct_trace(head)?;
    if raw.is_empty() {
        return None;
    }
    raw.into_iter()
        .map(|step| {
            let event = step.event();
            let location = project.fact_location(event)?;
            let message = match step.role() {
                EvidenceRole::Source => "flow source",
                EvidenceRole::Requirement => "flow requirement",
                EvidenceRole::Sink => "flow sink",
                EvidenceRole::Occurrence => "occurrence",
                _ => "evidence",
            };
            Some(EvidenceStep::new(step.role(), message.into(), location))
        })
        .collect()
}

fn fallback_trace(
    ev: &ClassificationEvidence,
    path: &ProjectRelativePath,
    range: &SourceRange,
) -> Vec<EvidenceStep> {
    vec![EvidenceStep::new(
        EvidenceRole::Occurrence,
        format!("{} of \"{}\"", ev.kind().as_str(), ev.symbol()),
        SourceLocation::new(path.clone(), range.clone()),
    )]
}
