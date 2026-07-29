use std::collections::BTreeMap;

use glass_lint_datastructures::{Position, SourceRange};

use crate::{
    AnalysisLimits, ParseDiagnostic, REPORT_VERSION,
    analysis::{
        ProjectSemanticModel, ResolvedLinkInput, project::projection::ProjectionOutcome,
        trace::TraceArena,
    },
    api::classification::{ClassificationResult, MatchedCapability, RuleIndex, TraceNodeId},
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
        let (mut files, parse_failure_codes) =
            Self::initialize_project_files(source_map, parse_diagnostics);

        let linking_start = std::time::Instant::now();
        let mut project = ProjectSemanticModel::link_with_limits(link_input, limits);
        for (path, code) in parse_failure_codes {
            project.record_parse_failure(path, &code);
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

    #[allow(clippy::too_many_lines)]
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
        let mut by_range: BTreeMap<SourceRange, Vec<usize>> = BTreeMap::new();
        for (ev_idx, evidence) in evidence_items.iter().enumerate() {
            for occurrence in &evidence.occurrences {
                let span = occurrence.span;
                if span.is_empty() {
                    continue;
                }
                let Ok(range) = lines.try_range(span) else {
                    continue;
                };
                by_range.entry(range).or_default().push(ev_idx);
            }
        }
        let entries: Vec<(SourceRange, Vec<usize>)> = by_range.into_iter().collect();
        let mut ranges: Vec<SourceRange> = entries.iter().map(|(r, _)| r.clone()).collect();
        crate::lint::ranges::remove_contained_ranges(&mut ranges);
        let label = capability.label();
        let severity = capability.severity();
        let mut groups: Vec<Vec<(usize, &SourceRange)>> = vec![Vec::new(); ranges.len()];
        let mut entry_cursor = 0usize;
        for (retained_idx, retained) in ranges.iter().enumerate() {
            while entry_cursor < entries.len() && entries[entry_cursor].0.end() < retained.start() {
                entry_cursor += 1;
            }
            let mut scan = entry_cursor;
            while scan < entries.len() && entries[scan].0.start() <= retained.end() {
                if retained.contains(&entries[scan].0) {
                    for ev_idx in &entries[scan].1 {
                        groups[retained_idx].push((*ev_idx, &entries[scan].0));
                    }
                }
                scan += 1;
            }
        }
        let arena = project.trace_arena().lock().unwrap();
        ranges
            .into_iter()
            .enumerate()
            .map(|(retained_idx, range)| {
                let mut traces: Vec<EvidenceTrace> = Vec::new();
                for (ev_idx, item_range) in &groups[retained_idx] {
                    let ev = &evidence_items[*ev_idx];
                    let occurrences = ev
                        .occurrences
                        .iter()
                        .filter(|o| lines.try_range(o.span).ok().as_ref() == Some(item_range))
                        .collect::<Vec<_>>();
                    if occurrences.is_empty() {
                        traces.push(EvidenceTrace::new(Self::fallback_trace(
                            ev, path, item_range,
                        )));
                        continue;
                    }
                    for occurrence in occurrences {
                        let steps = occurrence.trace.map_or_else(
                            || Some(Self::fallback_trace(ev, path, item_range)),
                            |trace_id| Self::resolve_trace(&arena, trace_id, project, path),
                        );
                        if let Some(s) = steps
                            && !s.is_empty()
                        {
                            traces.push(EvidenceTrace::new(s));
                        }
                    }
                }
                if traces.is_empty() {
                    traces.push(EvidenceTrace::new(vec![EvidenceStep::new(
                        EvidenceRole::Occurrence,
                        "evidence occurrence".into(),
                        SourceLocation::new(path.clone(), range.clone()),
                    )]));
                }
                let mut distinct_traces = Vec::with_capacity(traces.len());
                for trace in traces {
                    if !distinct_traces.contains(&trace) {
                        distinct_traces.push(trace);
                    }
                }
                let truncated = groups[retained_idx]
                    .iter()
                    .any(|(ev_idx, _)| evidence_items[*ev_idx].truncated);
                let certainty = if groups[retained_idx]
                    .iter()
                    .map(|(ev_idx, _)| evidence_items[*ev_idx].certainty)
                    .any(|certainty| certainty == MatchCertainty::Definite)
                {
                    MatchCertainty::Definite
                } else {
                    MatchCertainty::Possible
                };
                Finding::new(
                    rule_id.clone(),
                    label.to_string(),
                    severity,
                    SourceLocation::new(path.clone(), range),
                    EvidenceTraces::with_truncation(distinct_traces, truncated),
                    certainty,
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
        for (qe, role) in &raw {
            let location = project.fact_location(qe.module, qe.fact.0)?;
            let message = match role {
                EvidenceRole::Source => "flow source".into(),
                EvidenceRole::Requirement => "flow requirement".into(),
                EvidenceRole::Sink => "flow sink".into(),
                EvidenceRole::Occurrence => "occurrence".into(),
                _ => "evidence".into(),
            };
            steps.push(EvidenceStep::new(*role, message, location));
        }
        Some(steps)
    }

    /// Create a single-step fallback trace for evidence without arena traces.
    fn fallback_trace(
        ev: &crate::api::classification::ClassificationEvidence,
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
        BTreeMap<ProjectRelativePath, String>,
    ) {
        let mut files: BTreeMap<ProjectRelativePath, FileReport> = BTreeMap::new();
        let mut parse_failure_codes: BTreeMap<ProjectRelativePath, String> = BTreeMap::new();
        for source in source_map.values() {
            let path = source.path().clone();
            match parse_diagnostics.remove(&path) {
                Some(diagnostic) => {
                    parse_failure_codes.insert(path.clone(), diagnostic.code.as_str().to_owned());
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
            parse_failure_codes.insert(path.clone(), diagnostic.code.as_str().to_owned());
        }
        (files, parse_failure_codes)
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

        let trace_nodes = project
            .trace_arena()
            .lock()
            .map_or(0, |arena| arena.node_count());

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
