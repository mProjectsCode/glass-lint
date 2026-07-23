use std::{collections::BTreeMap, sync::Arc};

use crate::{
    AnalysisLimits, AnalysisReport, Diagnostic, EvidenceList, FileReport, Finding, ParseDiagnostic,
    Position, ProjectInputError, ProjectRelativePath, REPORT_VERSION, SourceLocation, SourceRange,
    analysis::{LocalArtifact, ProjectSemanticModel, project::projection::ProjectionOutcome},
    api::classification::{ClassificationResult, MatchedCapability, RuleIndex},
    diagnostic::SourceLineIndex,
    lint::catalog::RuleCatalog,
    project::{ModuleId, input::ValidatedProjectInput},
};

/// Outcome of linking and matching a resolved project, with phase timings.
pub struct ProjectAnalysis {
    pub report: AnalysisReport,
    pub linking: std::time::Duration,
    pub matching: std::time::Duration,
}

/// Report construction stage. Converts a linked and classified project into
/// an `AnalysisReport` with findings, evidence, and diagnostics.
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

    /// Link, classify, and assemble the report.
    pub fn finish(
        &self,
        input: ValidatedProjectInput,
        analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
        parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        limits: &AnalysisLimits,
    ) -> Result<ProjectAnalysis, ProjectInputError> {
        let (mut files, parse_failure_codes) =
            Self::initialize_project_files(&input, parse_diagnostics);

        tracing::debug!(
            target: "glass_lint::project::link",
            modules = analyzed.len(),
            resolutions = input.resolution_count(),
            "stage started"
        );
        let linking_start = std::time::Instant::now();
        let mut project = ProjectSemanticModel::link_with_limits(input, analyzed, limits)?;
        for (path, code) in parse_failure_codes {
            project.record_parse_failure(path, &code);
        }

        let linking = linking_start.elapsed();
        let link_counts = project.operation_counts(0);
        tracing::info!(
            target: "glass_lint::project::link",
            files = link_counts.files,
            requests = link_counts.requests,
            edges = link_counts.edges,
            elapsed = ?linking,
            "stage finished"
        );
        let matching_start = std::time::Instant::now();
        tracing::debug!(target: "glass_lint::project::matching", rules = self.enabled.len(), "stage started");
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
            files = report.operations.files,
            findings = summary.findings,
            evidence = report.operations.evidence,
            diagnostics = report.diagnostics.len() + summary.parse_diagnostics,
            elapsed = ?matching,
            "stage finished"
        );

        Ok(ProjectAnalysis {
            report,
            linking,
            matching,
        })
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
            let mut findings = self.project_findings_for_module(project, module, classification);
            findings.sort_by(|a, b| {
                a.location
                    .range
                    .start()
                    .line()
                    .cmp(&b.location.range.start().line())
                    .then_with(|| {
                        a.location
                            .range
                            .start()
                            .column()
                            .cmp(&b.location.range.start().column())
                    })
                    .then_with(|| a.rule_id.as_str().cmp(b.rule_id.as_str()))
            });
            findings.dedup();
            files.insert(
                module.path().clone(),
                FileReport {
                    path: module.path().clone(),
                    findings,
                    diagnostics: Vec::new(),
                },
            );
        }
    }

    fn project_findings_for_module(
        &self,
        project: &ProjectSemanticModel,
        module: &crate::analysis::ProjectModule,
        classification: &ClassificationResult,
    ) -> Vec<Finding> {
        let lines = &module.source_context().lines;
        let path = module.path();

        let mut by_rule: BTreeMap<RuleIndex, (Vec<Finding>, Vec<crate::Evidence>)> =
            BTreeMap::new();

        for capability in classification.capabilities() {
            let related: Vec<_> = capability
                .evidence()
                .iter()
                .flat_map(|evidence| &evidence.related)
                .filter_map(|related| {
                    let mut evidence =
                        project.fact_location(ModuleId::new(related.module), related.event)?;
                    evidence.message.clone_from(&related.symbol);
                    Some(evidence)
                })
                .collect();
            let cap_findings = self.findings_for_capability(capability, lines, path);

            let (rule_findings, rule_related) = by_rule.entry(capability.rule_index).or_default();
            rule_findings.extend(cap_findings);
            rule_related.extend(related);
        }

        let mut result: Vec<Finding> = Vec::new();
        for (_, (mut rule_findings, related)) in by_rule {
            if !related.is_empty() {
                let shared: Arc<[crate::Evidence]> = related.into();
                for finding in &mut rule_findings {
                    finding.set_shared_evidence(Arc::clone(&shared));
                }
            }
            result.append(&mut rule_findings);
        }
        result
    }

    fn findings_for_capability(
        &self,
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

        let mut by_range: BTreeMap<SourceRange, usize> = BTreeMap::new();
        for (ev_idx, evidence) in evidence_items.iter().enumerate() {
            for occurrence in &evidence.occurrences {
                let span = occurrence.span;
                if span.is_empty() {
                    continue;
                }
                let Ok(range) = lines.try_range(span) else {
                    continue;
                };
                by_range.entry(range).or_insert(ev_idx);
            }
        }

        let entries: Vec<(SourceRange, usize)> = by_range.into_iter().collect();

        let mut ranges: Vec<SourceRange> = entries.iter().map(|(r, _)| r.clone()).collect();
        crate::lint::ranges::remove_contained_ranges(&mut ranges);

        let label: Arc<str> = Arc::from(capability.label());
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
                    groups[retained_idx].push((entries[scan].1, &entries[scan].0));
                }
                scan += 1;
            }
        }

        ranges
            .into_iter()
            .enumerate()
            .map(|(retained_idx, range)| {
                let local_evidence: EvidenceList = groups[retained_idx]
                    .iter()
                    .map(|(ev_idx, item_range)| {
                        let ev = &evidence_items[*ev_idx];
                        crate::Evidence {
                            message: format!("{} of \"{}\"", ev.kind().as_str(), ev.symbol()),
                            count: ev.count,
                            evidence_truncated: ev.evidence_truncated,
                            location: Some(SourceLocation {
                                path: path.clone(),
                                range: (*item_range).clone(),
                            }),
                        }
                    })
                    .collect();
                Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: label.to_string(),
                    severity,
                    location: SourceLocation {
                        path: path.clone(),
                        range,
                    },
                    evidence: local_evidence,
                }
            })
            .collect()
    }

    fn initialize_project_files(
        input: &ValidatedProjectInput,
        mut parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
    ) -> (
        BTreeMap<ProjectRelativePath, FileReport>,
        BTreeMap<ProjectRelativePath, String>,
    ) {
        let mut files: BTreeMap<ProjectRelativePath, FileReport> = BTreeMap::new();
        let mut parse_failure_codes: BTreeMap<ProjectRelativePath, String> = BTreeMap::new();
        for source in input.source_map().values() {
            let path = source.path.clone();
            match parse_diagnostics.remove(&path) {
                Some(diagnostic) => {
                    parse_failure_codes.insert(path.clone(), diagnostic.code.as_str().to_owned());
                    files.insert(
                        path,
                        FileReport {
                            path: source.path.clone(),
                            findings: Vec::new(),
                            diagnostics: vec![Diagnostic::parse(source.path.clone(), diagnostic)],
                        },
                    );
                }
                None => {
                    files.insert(
                        path,
                        FileReport {
                            path: source.path.clone(),
                            findings: Vec::new(),
                            diagnostics: Vec::new(),
                        },
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
            diagnostic.location = Some(SourceLocation {
                path: path.clone(),
                range: SourceRange::new(
                    Position::new(1, 1).expect("one-based position"),
                    Position::new(1, 1).expect("one-based position"),
                )
                .expect("ordered source range"),
            });
            if let Some(file) = files.get_mut(&path) {
                file.diagnostics.push(Diagnostic::project(diagnostic));
            }
        }

        let mut diagnostics = Vec::new();
        for diagnostic in project.diagnostics().iter().cloned() {
            if let Some(path) = diagnostic
                .location
                .as_ref()
                .map(|location| location.path.clone())
            {
                if let Some(file) = files.get_mut(&path) {
                    file.diagnostics.push(Diagnostic::project(diagnostic));
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
            .map(|file| {
                file.findings
                    .iter()
                    .map(|finding| finding.evidence.len())
                    .sum::<usize>()
            })
            .sum();
        let is_partial = !project.is_complete();
        let mut operations = project.operation_counts(evidence);
        operations.effect_projections = outcome.effect_projections;
        AnalysisReport {
            schema_version: REPORT_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").into(),
            files: files.into_values().collect(),
            diagnostics,
            operations,
            completion: if is_partial {
                crate::ReportCompletion::Partial
            } else {
                crate::ReportCompletion::Complete
            },
        }
    }
}
