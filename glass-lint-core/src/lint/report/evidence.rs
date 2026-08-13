use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::SourceRange;

use crate::{
    analysis::{ProjectSemanticModel, display_span},
    api::classification::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, ClassificationResult,
        MatchedCapability,
    },
    diagnostic::SourceLineIndex,
    lint::{catalog::RuleCatalog, report::ProjectReportSession},
    project::{
        EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, FileReport, Finding,
        MatchCertainty, ModuleId, ProjectRelativePath, SourceLocation,
    },
};

#[derive(Clone, Copy, Debug)]
struct ResolvedEvidenceOccurrence<'a> {
    evidence: &'a ClassificationEvidence,
    occurrence: &'a ClassificationEvidenceOccurrence,
}

#[derive(Debug)]
struct EvidenceRangeEntry<'a> {
    range: SourceRange,
    occurrences: Vec<ResolvedEvidenceOccurrence<'a>>,
}

#[derive(Debug)]
struct FindingGroup<'a> {
    range: SourceRange,
    occurrences: Vec<ResolvedEvidenceOccurrence<'a>>,
}

impl<'a> FindingGroup<'a> {
    fn new(range: SourceRange) -> Self {
        Self {
            range,
            occurrences: Vec::new(),
        }
    }

    fn add_entry(&mut self, entry: &EvidenceRangeEntry<'a>) {
        if self.range.contains(&entry.range) {
            self.occurrences.extend(entry.occurrences.iter().copied());
        }
    }

    fn into_evidence(
        self,
        project: &ProjectSemanticModel,
        session: &ProjectReportSession,
        path: &ProjectRelativePath,
    ) -> Option<(SourceRange, EvidenceTraces, MatchCertainty)> {
        let Self { range, occurrences } = self;
        let mut traces = BTreeSet::new();
        let mut truncated = false;
        let mut certainty = MatchCertainty::Possible;
        for resolved in occurrences {
            truncated |= resolved.evidence.is_truncated();
            if resolved.evidence.certainty() == MatchCertainty::Definite {
                certainty = MatchCertainty::Definite;
            }
            let steps = resolved.occurrence.trace().map_or_else(
                || Some(fallback_trace(resolved.evidence, path, &range)),
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
                SourceLocation::new(path.clone(), range.clone()),
            )])
        {
            traces.insert(trace);
        }
        let evidence =
            EvidenceTraces::with_truncation(traces.into_iter().collect(), truncated).ok()?;
        Some((range, evidence, certainty))
    }
}

pub(super) fn populate_project_files(
    catalog: &RuleCatalog,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    classifications: &BTreeMap<ModuleId, ClassificationResult>,
    files: &mut BTreeMap<ProjectRelativePath, FileReport>,
) {
    for module in project.modules() {
        let Some(classification) = classifications.get(&module.id()) else {
            continue;
        };
        let findings = findings_for_module(catalog, project, session, module, classification);
        let findings = merge_duplicate_findings(findings);
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
                merged.push(previous.merge_duplicate(finding));
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
    catalog: &RuleCatalog,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    module: &crate::analysis::ProjectModule,
    classification: &ClassificationResult,
) -> Vec<Finding> {
    let lines = module.source_context().lines();
    let path = module.path();
    let mut findings = Vec::new();
    for capability in classification.capabilities() {
        findings.extend(findings_for_capability(
            catalog, project, session, capability, lines, path,
        ));
    }
    findings
}

#[derive(Debug)]
struct FindingRangeBuilder<'a> {
    entries: Vec<EvidenceRangeEntry<'a>>,
    retained_ranges: Vec<SourceRange>,
}

impl<'a> FindingRangeBuilder<'a> {
    fn new(evidence_items: &'a [ClassificationEvidence], lines: &SourceLineIndex) -> Self {
        let mut by_range: BTreeMap<SourceRange, Vec<ResolvedEvidenceOccurrence<'a>>> =
            BTreeMap::new();
        for evidence in evidence_items {
            for occurrence in evidence.occurrences() {
                let span =
                    display_span(lines, occurrence.span(), evidence.kind(), evidence.symbol());
                if span.is_empty() {
                    continue;
                }
                let Ok(range) = lines.try_range(span) else {
                    continue;
                };
                by_range
                    .entry(range)
                    .or_default()
                    .push(ResolvedEvidenceOccurrence {
                        evidence,
                        occurrence,
                    });
            }
        }

        let entries = by_range
            .into_iter()
            .map(|(range, occurrences)| EvidenceRangeEntry { range, occurrences })
            .collect::<Vec<_>>();
        let mut retained_ranges = entries
            .iter()
            .map(|entry| entry.range.clone())
            .collect::<Vec<_>>();
        crate::lint::ranges::remove_contained_ranges(&mut retained_ranges);
        Self {
            entries,
            retained_ranges,
        }
    }

    fn into_groups(self) -> Vec<FindingGroup<'a>> {
        let Self {
            entries,
            retained_ranges,
        } = self;
        let mut groups = Vec::with_capacity(retained_ranges.len());
        let mut entry_cursor = 0usize;
        for retained in retained_ranges {
            while entry_cursor < entries.len()
                && entries[entry_cursor].range.end() < retained.start()
            {
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
}

fn findings_for_capability(
    catalog: &RuleCatalog,
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    capability: &MatchedCapability,
    lines: &SourceLineIndex,
    path: &ProjectRelativePath,
) -> Vec<Finding> {
    let Some(rule_id) = catalog.rule_id(capability.rule_index()).cloned() else {
        return Vec::new();
    };
    let evidence_items = capability.evidence();
    if evidence_items.is_empty() {
        return Vec::new();
    }
    let groups = FindingRangeBuilder::new(evidence_items, lines).into_groups();
    groups
        .into_iter()
        .filter_map(|group| {
            let (range, evidence, certainty) = group.into_evidence(project, session, path)?;
            Finding::new(
                rule_id.clone(),
                capability.label().to_string(),
                capability.severity(),
                SourceLocation::new(path.clone(), range),
                evidence,
                certainty,
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
