use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::SourceRange;

use crate::{
    analysis::{ProjectSemanticModel, display_span},
    api::classification::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, ClassificationResult,
        MatchedCapability,
    },
    diagnostic::SourceLineIndex,
    lint::{
        catalog::RuleCatalog,
        report::{ProjectReportSession, files::ReportFiles},
    },
    project::{
        EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces, Finding, MatchCertainty,
        ModuleId, ProjectRelativePath, SourceLocation,
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

impl EvidenceRangeEntry<'_> {
    fn new(range: SourceRange) -> Self {
        Self {
            range,
            occurrences: Vec::new(),
        }
    }

    fn add_entry(&mut self, entry: &Self) {
        if self.range.contains(&entry.range) {
            self.occurrences.extend(entry.occurrences.iter().copied());
        }
    }

    fn into_evidence(
        self,
        renderer: &FindingRenderer<'_>,
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
                || {
                    Some(FindingRenderer::fallback_trace(
                        resolved.evidence,
                        path,
                        &range,
                    ))
                },
                |trace_id| renderer.resolve_trace(trace_id),
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

pub(super) struct FindingRenderer<'a> {
    catalog: &'a RuleCatalog,
    project: &'a ProjectSemanticModel,
    session: &'a ProjectReportSession,
}

impl<'a> FindingRenderer<'a> {
    pub(super) fn new(
        catalog: &'a RuleCatalog,
        project: &'a ProjectSemanticModel,
        session: &'a ProjectReportSession,
    ) -> Self {
        Self {
            catalog,
            project,
            session,
        }
    }

    pub(super) fn populate_project_files(
        &self,
        classifications: &BTreeMap<ModuleId, ClassificationResult>,
        files: &mut ReportFiles,
    ) {
        for module in self.project.modules() {
            let Some(classification) = classifications.get(&module.id()) else {
                continue;
            };
            let findings = self.findings_for_module(module, classification);
            files.replace_findings(module.path(), merge_duplicate_findings(findings));
        }
    }

    fn findings_for_module(
        &self,
        module: &crate::analysis::ProjectModule,
        classification: &ClassificationResult,
    ) -> Vec<Finding> {
        let lines = module.source_context().lines();
        let path = module.path();
        let mut findings = Vec::new();
        for capability in classification.capabilities() {
            findings.extend(self.findings_for_capability(capability, lines, path));
        }
        findings
    }

    fn findings_for_capability(
        &self,
        capability: &MatchedCapability,
        lines: &SourceLineIndex,
        path: &ProjectRelativePath,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog.rule_id(capability.rule_index()).cloned() else {
            return Vec::new();
        };
        let evidence_items = capability.evidence();
        if evidence_items.is_empty() {
            return Vec::new();
        }
        let entries = FindingRangeBuilder::new(evidence_items, lines).into_entries();
        entries
            .into_iter()
            .filter_map(|entry| {
                let (range, evidence, certainty) = entry.into_evidence(self, path)?;
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
        &self,
        head: crate::analysis::trace::TraceNodeId,
    ) -> Option<Vec<EvidenceStep>> {
        let raw = self.session.reconstruct_trace(head)?;
        if raw.is_empty() {
            return None;
        }
        raw.into_iter()
            .map(|step| {
                let event = step.event();
                let location = self.project.fact_location(event)?;
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

#[derive(Debug)]
struct FindingRangeBuilder<'a> {
    entries: Vec<EvidenceRangeEntry<'a>>,
    retained_indices: Vec<usize>,
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
        let retained_indices = retained_indices(&entries);
        Self {
            entries,
            retained_indices,
        }
    }

    fn into_entries(self) -> Vec<EvidenceRangeEntry<'a>> {
        let Self {
            entries,
            retained_indices,
        } = self;
        let mut groups = Vec::with_capacity(retained_indices.len());
        let mut entry_cursor = 0usize;
        for retained_index in retained_indices {
            let retained = entries[retained_index].range.clone();
            while entry_cursor < entries.len()
                && entries[entry_cursor].range.end() < retained.start()
            {
                entry_cursor += 1;
            }
            let mut group = EvidenceRangeEntry::new(retained);
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

fn retained_indices(entries: &[EvidenceRangeEntry<'_>]) -> Vec<usize> {
    let mut retained_indices = (0..entries.len()).collect::<Vec<_>>();
    retained_indices.sort_by(|left, right| {
        let left = &entries[*left].range;
        let right = &entries[*right].range;
        (left.start().line(), left.start().column())
            .cmp(&(right.start().line(), right.start().column()))
            .then_with(|| {
                (right.end().line(), right.end().column())
                    .cmp(&(left.end().line(), left.end().column()))
            })
    });
    let mut enclosing_end = None;
    retained_indices.retain(|index| {
        let end = (
            entries[*index].range.end().line(),
            entries[*index].range.end().column(),
        );
        if enclosing_end.is_some_and(|outer| end <= outer) {
            return false;
        }
        enclosing_end = Some(end);
        true
    });
    retained_indices
}

#[cfg(test)]
mod tests;
