use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{Position, SourceRange};

use crate::{
    AnalysisLimits, ParseDiagnostic, REPORT_VERSION,
    analysis::{
        ProjectSemanticModel, ResolvedLinkInput, display_span,
        project::projection::ProjectionOutcome, trace::TraceArena,
    },
    api::classification::{
        ClassificationEvidence, ClassificationEvidenceOccurrence, ClassificationResult,
        MatchedCapability, RuleIndex, TraceNodeId,
    },
    diagnostic::SourceLineIndex,
    lint::catalog::RuleCatalog,
    project::{
        AnalysisReport, Diagnostic, EvidenceRole, EvidenceStep, EvidenceTrace, EvidenceTraces,
        FileReport, Finding, MatchCertainty, ModuleId, ProjectRelativePath, ReportCompletion,
        SourceFile, SourceLocation,
    },
};

pub struct ProjectAnalysis {
    pub report: AnalysisReport,
    pub linking: std::time::Duration,
    pub matching: std::time::Duration,
}

pub struct ReportAssembly<'a> {
    catalog: &'a RuleCatalog,
    enabled: &'a [RuleIndex],
    evidence_limit: usize,
}

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
        Some((evidence, evidence.occurrences.get(self.occurrence)?))
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
            .any(|(evidence, _)| evidence.truncated)
    }

    fn certainty(&self, evidence_items: &[ClassificationEvidence]) -> MatchCertainty {
        if self
            .occurrences
            .iter()
            .filter_map(|reference| reference.resolve(evidence_items))
            .any(|(evidence, _)| evidence.certainty == MatchCertainty::Definite)
        {
            MatchCertainty::Definite
        } else {
            MatchCertainty::Possible
        }
    }
}

impl<'a> ReportAssembly<'a> {
    pub fn new(catalog: &'a RuleCatalog, enabled: &'a [RuleIndex], evidence_limit: usize) -> Self {
        Self {
            catalog,
            enabled,
            evidence_limit,
        }
    }

    pub fn finish(
        &self,
        source_map: &BTreeMap<ProjectRelativePath, SourceFile>,
        link_input: ResolvedLinkInput,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> ProjectAnalysis {
        let (mut files, parse_failures) =
            Self::initialize_project_files(source_map, parse_diagnostics);

        let linking_start = std::time::Instant::now();
        let mut project = ProjectSemanticModel::link_with_limits(link_input, limits);
        for (path, failure) in parse_failures {
            project.record_parse_failure(path, failure);
        }
        let linking = linking_start.elapsed();
        let link_counts = project.operation_counts(0);

        tracing::info!(
            target: "glass_lint::project::link",
            files = link_counts.files(), requests = link_counts.requests(),
            edges = link_counts.edges(), elapsed = ?linking, "stage finished"
        );

        let matching_start = std::time::Instant::now();
        let (classifications, projection_outcome) = project.classify_with_evidence_limit(
            self.catalog.compiled(),
            self.enabled,
            self.evidence_limit,
        );

        project.record_flow_exhaustion(&projection_outcome);
        let matching = matching_start.elapsed();
        self.populate_project_files(&project, &classifications, &mut files);

        let diagnostics = Self::attach_project_diagnostics(&project, &mut files);
        let report =
            Self::assemble_project_report(&project, files, diagnostics, &projection_outcome);
        let summary = report.summary();

        tracing::info!(
            target: "glass_lint::project::matching",
            files = report.operations().files(), findings = summary.findings(),
            evidence = report.operations().evidence(),
            diagnostics = report.diagnostics().len() + summary.parse_diagnostics(),
            elapsed = ?matching, "stage finished"
        );

        ProjectAnalysis {
            report,
            linking,
            matching,
        }
    }

    fn populate_project_files(
        &self,
        project: &ProjectSemanticModel,
        classifications: &BTreeMap<ModuleId, ClassificationResult>,
        files: &mut BTreeMap<ProjectRelativePath, FileReport>,
    ) {
        for module in project.modules() {
            let Some(classification) = classifications.get(&module.id()) else {
                continue;
            };
            let findings = self.project_findings_for_module(project, module, classification);
            let mut findings = Self::merge_duplicate_findings(findings);
            findings.sort_by(Self::compare_findings);
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
        findings.sort_by(Self::compare_findings);
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

    fn project_findings_for_module(
        &self,
        project: &ProjectSemanticModel,
        module: &crate::analysis::ProjectModule,
        classification: &ClassificationResult,
    ) -> Vec<Finding> {
        let lines = &module.source_context().lines;
        let path = module.path();
        let mut rule_findings: BTreeMap<RuleIndex, Vec<Finding>> = BTreeMap::new();
        for capability in classification.capabilities() {
            let cap_findings = self.findings_for_capability(project, capability, lines, path);
            rule_findings
                .entry(capability.rule_index)
                .or_default()
                .extend(cap_findings);
        }
        rule_findings.into_values().flatten().collect()
    }

    fn range_entries(
        evidence_items: &[ClassificationEvidence],
        lines: &SourceLineIndex,
    ) -> Vec<EvidenceRangeEntry> {
        let mut by_range: BTreeMap<SourceRange, Vec<EvidenceOccurrenceRef>> = BTreeMap::new();
        for (evidence_idx, evidence) in evidence_items.iter().enumerate() {
            for (occurrence_idx, occurrence) in evidence.occurrences.iter().enumerate() {
                let span = display_span(
                    lines,
                    occurrence.span,
                    evidence.kind,
                    evidence.symbol.as_str(),
                );
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
        let mut ranges: Vec<SourceRange> =
            entries.iter().map(|entry| entry.range.clone()).collect();
        crate::lint::ranges::remove_contained_ranges(&mut ranges);
        let mut groups = Vec::with_capacity(ranges.len());
        let mut entry_cursor = 0usize;
        for retained in ranges {
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

    fn findings_for_capability(
        &self,
        project: &ProjectSemanticModel,
        capability: &MatchedCapability,
        lines: &SourceLineIndex,
        path: &ProjectRelativePath,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog.rule_id(capability.rule_index).cloned() else {
            return Vec::new();
        };
        let evidence_items = capability.evidence();
        if evidence_items.is_empty() {
            return Vec::new();
        }
        let label = capability.label();
        let severity = capability.severity();
        let entries = Self::range_entries(evidence_items, lines);
        let groups = Self::finding_groups(&entries);
        let arena = project.trace_arena();
        groups
            .into_iter()
            .map(|group| {
                let mut traces = BTreeSet::new();
                for reference in &group.occurrences {
                    let Some((ev, occurrence)) = reference.resolve(evidence_items) else {
                        continue;
                    };
                    let steps = occurrence.trace.map_or_else(
                        || Some(Self::fallback_trace(ev, path, &group.range)),
                        |trace_id| Self::resolve_trace(arena, trace_id, project, path),
                    );
                    if let Some(s) = steps
                        && !s.is_empty()
                    {
                        traces.insert(EvidenceTrace::new(s));
                    }
                }
                if traces.is_empty() {
                    traces.insert(EvidenceTrace::new(vec![EvidenceStep::new(
                        EvidenceRole::Occurrence,
                        "evidence occurrence".into(),
                        SourceLocation::new(path.clone(), group.range.clone()),
                    )]));
                }
                Finding::new(
                    rule_id.clone(),
                    label.to_string(),
                    severity,
                    SourceLocation::new(path.clone(), group.range.clone()),
                    EvidenceTraces::with_truncation(
                        traces.into_iter().collect(),
                        group.is_truncated(evidence_items),
                    ),
                    group.certainty(evidence_items),
                )
            })
            .collect()
    }

    /// Resolve a trace chain from the arena into evidence steps.
    /// Returns None if any required step cannot be resolved.
    fn resolve_trace(
        arena: &TraceArena,
        head: TraceNodeId,
        project: &ProjectSemanticModel,
        _path: &ProjectRelativePath,
    ) -> Option<Vec<EvidenceStep>> {
        let raw = arena.reconstruct_trace(head);
        if raw.is_empty() {
            return None;
        }
        let mut steps = Vec::with_capacity(raw.len());
        for step in &raw {
            let event = step.event();
            let role = step.role();
            let location = project.fact_location(event.module, event.fact)?;
            let message = match role {
                EvidenceRole::Source => "flow source".into(),
                EvidenceRole::Requirement => "flow requirement".into(),
                EvidenceRole::Sink => "flow sink".into(),
                EvidenceRole::Occurrence => "occurrence".into(),
                _ => "evidence".into(),
            };
            steps.push(EvidenceStep::new(role, message, location));
        }
        Some(steps)
    }

    /// Create a single-step fallback trace for evidence without arena traces.
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

    fn initialize_project_files(
        source_map: &BTreeMap<ProjectRelativePath, SourceFile>,
        mut parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
    ) -> (
        BTreeMap<ProjectRelativePath, FileReport>,
        BTreeMap<ProjectRelativePath, crate::parse::ParseFailureKind>,
    ) {
        let mut files: BTreeMap<ProjectRelativePath, FileReport> = BTreeMap::new();
        let mut parse_failures = BTreeMap::new();
        for source in source_map.values() {
            let path = source.path().clone();
            match parse_diagnostics.remove(&path) {
                Some(diagnostic) => {
                    parse_failures.insert(path.clone(), diagnostic.failure);
                    files.insert(
                        path,
                        FileReport::new(
                            source.path().clone(),
                            Vec::new(),
                            vec![Diagnostic::parse(source.path().clone(), diagnostic)],
                        ),
                    );
                }
                None => {
                    files.insert(
                        path,
                        FileReport::new(source.path().clone(), Vec::new(), Vec::new()),
                    );
                }
            }
        }
        for (path, diagnostic) in parse_diagnostics {
            parse_failures.insert(path.clone(), diagnostic.failure);
        }
        (files, parse_failures)
    }

    fn attach_project_diagnostics(
        project: &ProjectSemanticModel,
        files: &mut BTreeMap<ProjectRelativePath, FileReport>,
    ) -> Vec<Diagnostic> {
        let (status_files, status_project) = project.status_diagnostics();
        for (path, mut diagnostic) in status_files {
            diagnostic.set_location(Some(SourceLocation::new(
                path.clone(),
                SourceRange::new(
                    Position::new(1, 1).expect("one-based position"),
                    Position::new(1, 1).expect("one-based position"),
                )
                .expect("ordered source range"),
            )));
            if let Some(file) = files.get_mut(&path) {
                file.diagnostics_mut().push(Diagnostic::project(diagnostic));
            }
        }

        let mut diagnostics = Vec::new();
        for diagnostic in project.diagnostics().iter().cloned() {
            if let Some(path) = diagnostic.location().map(|l| l.path().clone()) {
                if let Some(file) = files.get_mut(&path) {
                    file.diagnostics_mut().push(Diagnostic::project(diagnostic));
                }
            } else {
                diagnostics.push(Diagnostic::project(diagnostic));
            }
        }

        diagnostics.extend(status_project.into_iter().map(Diagnostic::project));
        diagnostics.sort_by(|left, right| left.code().cmp(right.code()));
        diagnostics
    }

    fn assemble_project_report(
        project: &ProjectSemanticModel,
        files: BTreeMap<ProjectRelativePath, FileReport>,
        diagnostics: Vec<Diagnostic>,
        outcome: &ProjectionOutcome,
    ) -> AnalysisReport {
        let evidence = files
            .values()
            .map(|f| {
                f.findings()
                    .iter()
                    .map(|finding| {
                        finding
                            .evidence()
                            .traces()
                            .iter()
                            .map(|t| t.steps().len())
                            .sum::<usize>()
                    })
                    .sum::<usize>()
            })
            .sum();

        let rendered_traces = files
            .values()
            .flat_map(FileReport::findings)
            .map(|finding| finding.evidence().traces().len())
            .sum();

        let is_partial = !project.is_complete();
        let mut operations = project.operation_counts(evidence);
        operations.set_effect_projections(outcome.effect_projections);

        let trace_nodes = project.trace_arena().node_count();

        operations.set_path_metrics(
            outcome.max_live_alternatives,
            trace_nodes,
            outcome.trace_heads,
            outcome.coalescing_comparisons,
            outcome.fixed_point_iterations,
            rendered_traces,
        );

        AnalysisReport::new(
            REPORT_VERSION,
            env!("CARGO_PKG_VERSION").into(),
            files.into_values().collect(),
            diagnostics,
            operations,
            if is_partial {
                ReportCompletion::Partial
            } else {
                ReportCompletion::Complete
            },
        )
    }
}
