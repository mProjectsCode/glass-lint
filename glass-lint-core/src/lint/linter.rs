use std::{collections::BTreeMap, sync::Arc};

use crate::{
    AnalysisReport, Environment, ProjectInput, ProjectInputError, ProviderCatalogError,
    REPORT_VERSION, RuleId,
    analysis::{LocalArtifact, ProjectSemanticModel, project::projection::ProjectionOutcome},
    api::classification::ClassificationResult,
    lint::{
        catalog::RuleCatalog,
        selection::{LintConfigError, RuleBaseline, RuleSelection, RuleState},
    },
    project::{ModuleId, input::ValidatedProjectInput},
};

type AnalyzedModules = BTreeMap<crate::ProjectRelativePath, LocalArtifact>;

/// Outcome of linking and matching a resolved project, with phase timings
/// so `glass-lint-project` can observe pipeline boundaries without parsing
/// a positional tuple.
pub struct ProjectAnalysis {
    pub report: AnalysisReport,
    pub linking: std::time::Duration,
    pub matching: std::time::Duration,
}

/// Caller-supplied input to linter construction. Validation occurs in
/// [`Linter::new`].
#[derive(Clone, Debug)]
pub struct LinterConfig {
    /// Provider rule catalogs to combine into the unified analysis catalog.
    catalogs: Vec<RuleCatalog>,
    /// Host environment for global and global-object lookups.
    environment: Environment,
    /// Baseline and per-rule overrides for the combined catalog.
    selection: RuleSelection,
    /// Parser and semantic operation bounds.
    limits: crate::AnalysisLimits,
}

impl LinterConfig {
    pub fn new(catalogs: Vec<RuleCatalog>, environment: Environment) -> Self {
        Self {
            catalogs,
            environment,
            selection: RuleSelection::default(),
            limits: crate::AnalysisLimits::default(),
        }
    }

    #[must_use]
    pub fn with_rules(mut self, selection: RuleSelection) -> Self {
        self.selection = selection;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: crate::AnalysisLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn selection(&self) -> &RuleSelection {
        &self.selection
    }
}

/// Immutable catalog plus sorted enabled-rule indexes for lint execution.
///
/// The linter owns the combined rule catalog, host environment, enabled-rule
/// set, analysis limits, and a shared bounded artifact cache. It is `Send`
/// and `Sync` and can be cloned cheaply (the cache handle is `Arc<Mutex>`).
pub struct Linter {
    /// Validated rule catalog and compiled matcher plans.
    catalog: RuleCatalog,
    /// Host environment used during semantic fact construction.
    environment: Environment,
    /// Enabled rule indexes in deterministic catalog order.
    enabled: Vec<crate::api::classification::RuleIndex>,
    /// Parser and semantic operation bounds.
    limits: crate::AnalysisLimits,
    /// Shared bounded cache of successfully lowered artifacts.
    artifact_cache: crate::analysis::ArtifactCacheHandle,
}

impl Clone for Linter {
    fn clone(&self) -> Self {
        Self {
            catalog: self.catalog.clone(),
            environment: self.environment.clone(),
            enabled: self.enabled.clone(),
            limits: self.limits.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}

impl Linter {
    /// Starts a deterministic project collection session.
    pub fn begin_project(
        &self,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<crate::ProjectCollection<'_>, ProjectInputError> {
        crate::ProjectCollection::new(self, root)
    }

    /// Construct a linter from validated catalogs, environment, rule
    /// selection, and analysis limits. Catalogs are combined into one
    /// unified catalog (rejecting duplicate fully-qualified IDs), rule
    /// overrides are applied in declaration order, and limits are validated.
    pub fn new(config: LinterConfig) -> Result<Self, LintConfigError> {
        let catalog = RuleCatalog::combine(config.catalogs).map_err(|error| match error {
            ProviderCatalogError::InvalidRule(id, _) => {
                LintConfigError::DuplicateRule(RuleId::parse(id).expect("catalog IDs validated"))
            }
            ProviderCatalogError::InvalidRuleId(id) => LintConfigError::InvalidSelector(id),
        })?;
        config.selection.validate_against(&catalog)?;
        let mut enabled = Vec::new();
        for (index, rule_id) in catalog.rule_ids().iter().enumerate() {
            let baseline = match config.selection.baseline() {
                RuleBaseline::All => true,
                RuleBaseline::None => false,
                RuleBaseline::MinimumConfidence(confidence) => {
                    catalog.records[index].confidence as u8 <= confidence as u8
                }
            };
            let mut state = baseline;
            for override_ in config.selection.overrides() {
                if override_.matches(rule_id.as_str()) {
                    state = override_.state() == RuleState::Enabled;
                }
            }
            if state {
                enabled.push(crate::api::classification::RuleIndex::new(index));
            }
        }
        // Limits are guaranteed valid by construction through
        // `AnalysisLimits::new` or `Default`; no re-validation needed.
        Ok(Self {
            catalog,
            environment: config.environment,
            enabled,
            limits: config.limits,
            artifact_cache: crate::analysis::ArtifactCacheHandle::default(),
        })
    }

    #[must_use]
    /// Borrow the validated catalog.
    pub fn catalog(&self) -> &RuleCatalog {
        &self.catalog
    }

    /// Returns the enabled rule IDs in deterministic catalog order.
    #[must_use]
    pub fn enabled_rule_ids(&self) -> Vec<RuleId> {
        self.enabled
            .iter()
            .filter_map(|&index| self.catalog.rule_id(index).cloned())
            .collect()
    }

    /// Borrow the validated parser and semantic safety limits.
    pub fn analysis_limits(&self) -> &crate::AnalysisLimits {
        &self.limits
    }

    /// Borrow the complete host environment used by semantic analysis.
    pub fn analysis_environment(&self) -> &Environment {
        &self.environment
    }

    pub(crate) fn artifact_cache_handle(&self) -> crate::analysis::ArtifactCacheHandle {
        self.artifact_cache.clone()
    }

    /// Lints an in-memory project using explicit, already-classified
    /// resolution results.  Filesystem loading belongs to the project crate.
    ///
    /// ```
    /// use glass_lint_core::{
    ///     Environment, Linter, LinterConfig, ProjectInput, RuleCatalog, SourceFile,
    /// };
    ///
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let report = linter
    ///     .lint_project(ProjectInput {
    ///         root: ".".into(),
    ///         sources: vec![SourceFile::new("main.js", "").unwrap()],
    ///         resolutions: vec![],
    ///     })
    ///     .unwrap();
    /// assert_eq!(report.files.len(), 1);
    /// ```
    pub fn lint_project(&self, input: ProjectInput) -> Result<AnalysisReport, ProjectInputError> {
        let validated = input.admit()?;
        let file_count = validated.source_count();
        let resolution_count = validated.resolution_count();

        tracing::info!(
            target: "glass_lint::project",
            files = file_count,
            resolutions = resolution_count,
            "project analysis started"
        );
        let (root, sources, resolutions, _module_ids) = validated.into_parts();
        let mut collection = self.begin_project(root)?;
        for (path, source) in sources {
            collection.admit_validated_source(source)?;
            collection.analyze_source_at_path(&path)?;
        }
        let local = collection.finish_local();
        local
            .resolve(resolutions)
            .and_then(crate::ResolvedProject::finish)
    }

    /// Analyze one in-memory source through the canonical project session.
    ///
    /// A snippet is a project containing one source. This convenience method
    /// returns the same source-free [`AnalysisReport`] shape as
    /// [`Self::lint_project`].
    ///
    /// ```
    /// use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog};
    ///
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let report = linter.lint_snippet("", "snippet.js").unwrap();
    /// assert_eq!(report.files[0].path.as_str(), "snippet.js");
    /// ```
    pub fn lint_snippet(
        &self,
        source: &str,
        filename: &str,
    ) -> Result<AnalysisReport, ProjectInputError> {
        let filename = crate::ProjectRelativePath::new(filename)?;
        let mut collection = self.begin_project(".")?;
        collection.analyze_source(crate::SourceFile::new(filename.to_string(), source)?)?;
        collection.finish_local().resolve([])?.finish()
    }

    /// Finish the canonical project analysis and expose phase timings as
    /// observers of the same report-producing path.
    pub(crate) fn finish_analyzed_project(
        &self,
        input: ValidatedProjectInput,
        analyzed: AnalyzedModules,
        parse_diagnostics: BTreeMap<crate::ProjectRelativePath, crate::ParseDiagnostic>,
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
        let mut project = ProjectSemanticModel::link_with_limits(input, analyzed, &self.limits)?;
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
            &self.enabled,
            self.limits.evidence_items(),
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
        files: &mut BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
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
                crate::FileReport {
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
    ) -> Vec<crate::Finding> {
        let lines = &module.source_context().lines;
        let path = module.path();

        let mut by_rule: BTreeMap<
            crate::api::classification::RuleIndex,
            (Vec<crate::Finding>, Vec<crate::Evidence>),
        > = BTreeMap::new();

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

        let mut result: Vec<crate::Finding> = Vec::new();
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

    fn initialize_project_files(
        input: &ValidatedProjectInput,
        mut parse_diagnostics: BTreeMap<crate::ProjectRelativePath, crate::ParseDiagnostic>,
    ) -> (
        BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
        BTreeMap<crate::ProjectRelativePath, String>,
    ) {
        let mut files: BTreeMap<crate::ProjectRelativePath, crate::FileReport> = BTreeMap::new();
        let mut parse_failure_codes: BTreeMap<crate::ProjectRelativePath, String> = BTreeMap::new();
        for source in input.source_map().values() {
            let path = source.path.clone();
            match parse_diagnostics.remove(&path) {
                Some(diagnostic) => {
                    parse_failure_codes.insert(path.clone(), diagnostic.code.as_str().to_owned());
                    files.insert(
                        path,
                        crate::FileReport {
                            path: source.path.clone(),
                            findings: Vec::new(),
                            diagnostics: vec![crate::Diagnostic::parse(
                                source.path.clone(),
                                diagnostic,
                            )],
                        },
                    );
                }
                None => {
                    files.insert(
                        path,
                        crate::FileReport {
                            path: source.path.clone(),
                            findings: Vec::new(),
                            diagnostics: Vec::new(),
                        },
                    );
                }
            }
        }
        // Any remaining parse diagnostics (keys not in the source map)
        // are consumed for status recording rather than leaked.
        for (path, diagnostic) in parse_diagnostics {
            parse_failure_codes.insert(path.clone(), diagnostic.code.as_str().to_owned());
        }
        (files, parse_failure_codes)
    }

    fn attach_project_diagnostics(
        project: &ProjectSemanticModel,
        files: &mut BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
    ) -> Vec<crate::Diagnostic> {
        let (status_files, status_project) = project.status_diagnostics();
        for (path, mut diagnostic) in status_files {
            diagnostic.location = Some(crate::SourceLocation {
                path: path.clone(),
                range: crate::SourceRange::new(
                    crate::Position::new(1, 1).expect("one-based position"),
                    crate::Position::new(1, 1).expect("one-based position"),
                )
                .expect("ordered source range"),
            });
            if let Some(file) = files.get_mut(&path) {
                file.diagnostics
                    .push(crate::Diagnostic::project(diagnostic));
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
                    file.diagnostics
                        .push(crate::Diagnostic::project(diagnostic));
                }
            } else {
                diagnostics.push(crate::Diagnostic::project(diagnostic));
            }
        }
        diagnostics.extend(status_project.into_iter().map(crate::Diagnostic::project));
        diagnostics.sort_by(|left, right| left.code().cmp(right.code()));
        diagnostics
    }

    fn assemble_project_report(
        project: &ProjectSemanticModel,
        files: BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
        diagnostics: Vec<crate::Diagnostic>,
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

#[cfg(test)]
mod tests {
    use crate::{
        Environment, LintConfigError, Linter, LinterConfig, Position, RuleBaseline, RuleCatalog,
        RuleOverride, RuleSelection, RuleState, SourceRange,
        lint::ranges::remove_contained_ranges,
        rules::{Confidence, MatcherDecl, Rule, Severity},
    };

    #[test]
    fn range_sweep_removes_large_nested_and_duplicate_sets() {
        let mut ranges = (1..=5_000)
            .map(|column| {
                SourceRange::new(
                    Position::new(1, column).unwrap(),
                    Position::new(2, 5_001 - column).unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        ranges.push(ranges[0].clone());

        remove_contained_ranges(&mut ranges);

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start().column(), 1);
    }

    #[test]
    fn findings_are_sorted_without_cloning_rule_ids() {
        let rule = Rule::builder("network.request")
            .description("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .declaration(
                MatcherDecl::builder()
                    .call_global("fetch")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap();
        let mut environment = Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = Linter::new(LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        ))
        .unwrap();

        let report = linter
            .lint_snippet("fetch('/b'); fetch('/a');", "sort.js")
            .unwrap();
        // Findings should be sorted by line, then column, then rule ID.
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
        assert_eq!(
            report.files[0].findings[0].location.range.start().column(),
            1
        );
        assert_eq!(
            report.files[0].findings[1].location.range.start().column(),
            14
        );
    }

    #[test]
    fn classify_with_evidence_limit_binds_record_once() {
        let rule = Rule::builder("network.request")
            .description("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .declaration(
                MatcherDecl::builder()
                    .call_global("fetch")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap();
        let mut environment = Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = Linter::new(LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        ))
        .unwrap();

        let report = linter
            .lint_snippet("fetch('/a'); fetch('/b');", "classify.js")
            .unwrap();
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(
            report.files[0].findings[0].rule_id.as_str(),
            "test:network.request"
        );
    }

    #[test]
    fn missing_selected_rule_fails_closed() {
        let selection = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("unknown:missing", RuleState::Enabled).unwrap());
        let result = Linter::new(
            LinterConfig::new(
                vec![RuleCatalog::new("test", vec![]).unwrap()],
                Environment::default(),
            )
            .with_rules(selection),
        );
        assert!(matches!(result, Err(LintConfigError::UnknownRule(_))));
    }
}
