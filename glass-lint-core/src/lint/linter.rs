use std::collections::{BTreeMap, BTreeSet};

use super::catalog::RuleCatalog;
use crate::{
    AnalysisReport, AnalysisSession, CoreConfig, Environment, ProjectInput, ProjectInputError,
    REPORT_VERSION, RuleCatalogError, RuleId,
    analysis::{LocalArtifact, ProjectSemanticModel},
    api::classification::ApiClassificationResult,
    project::ModuleId,
};

type AnalyzedModules = BTreeMap<crate::ProjectRelativePath, LocalArtifact>;

struct ProjectFileState {
    sources: BTreeMap<crate::ProjectRelativePath, String>,
    files: BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
    parse_paths: Vec<(crate::ProjectRelativePath, String)>,
}

fn initialize_project_files(
    input: &ProjectInput,
    parse_diagnostics: BTreeMap<crate::ProjectRelativePath, crate::ParseDiagnostic>,
) -> ProjectFileState {
    let sources = input
        .sources
        .iter()
        .map(|source| (source.path.clone(), source.source.clone()))
        .collect::<BTreeMap<_, _>>();
    let parse_paths = parse_diagnostics
        .iter()
        .map(|(path, diagnostic)| (path.clone(), diagnostic.code.as_str().to_owned()))
        .collect::<Vec<_>>();
    let mut files = sources
        .keys()
        .map(|path| {
            (
                path.clone(),
                crate::FileReport {
                    path: path.clone(),
                    findings: Vec::new(),
                    diagnostics: Vec::new(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (path, diagnostic) in parse_diagnostics {
        let normalized = path;
        files.insert(
            normalized.clone(),
            crate::FileReport {
                path: normalized.clone(),
                findings: Vec::new(),
                diagnostics: vec![crate::Diagnostic::parse(normalized, diagnostic)],
            },
        );
    }
    ProjectFileState {
        sources,
        files,
        parse_paths,
    }
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
    AnalysisReport {
        schema_version: REPORT_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        files: files.into_values().collect(),
        diagnostics,
        operations: project.operation_counts(evidence),
        completion: if is_partial {
            crate::ReportCompletion::Partial
        } else {
            crate::ReportCompletion::Complete
        },
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Configuration failure when selecting rules for a linter.
pub enum LintConfigError {
    /// A requested fully-qualified rule ID is absent from the catalog.
    UnknownRule(RuleId),
    /// Safety limits are invalid.
    InvalidLimits(String),
}

impl std::fmt::Display for LintConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(f, "unknown rule `{id}`"),
            Self::InvalidLimits(message) => write!(f, "invalid resource limits: {message}"),
        }
    }
}

impl std::error::Error for LintConfigError {}

/// Immutable catalog plus sorted enabled-rule indexes for lint execution.
pub struct Linter {
    /// Validated rule catalog and compiled matcher plans.
    catalog: RuleCatalog,
    /// Enabled rule indexes in deterministic order.
    enabled: Vec<crate::api::classification::RuleIndex>,
    limits: crate::AnalysisLimits,
    artifact_cache: crate::analysis::ArtifactCacheHandle,
}

impl Clone for Linter {
    fn clone(&self) -> Self {
        Self {
            catalog: self.catalog.clone(),
            enabled: self.enabled.clone(),
            limits: self.limits.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}

impl Linter {
    #[cfg(test)]
    fn lint(&self, source: &str, filename: &str) -> AnalysisReport {
        self.lint_snippet(source, filename)
            .expect("test fixture path is valid")
    }

    /// Starts a deterministic project collection session.
    pub fn begin_analysis(
        &self,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<AnalysisSession<'_>, ProjectInputError> {
        AnalysisSession::new(self, root)
    }

    /// Apply provider-neutral engine configuration to this linter.
    pub fn configured(self, config: &CoreConfig) -> Result<Self, LintConfigError> {
        config
            .limits
            .validate()
            .map_err(LintConfigError::InvalidLimits)?;
        if let Some(rules) = &config.rules {
            let mut linter = Self::with_rules(self.catalog, rules.clone())?;
            linter.limits = config.limits.clone();
            linter.artifact_cache = self.artifact_cache;
            Ok(linter)
        } else {
            let mut linter = self;
            linter.limits = config.limits.clone();
            Ok(linter)
        }
    }

    #[must_use]
    /// Construct a linter with every catalog rule enabled.
    pub fn new(catalog: RuleCatalog) -> Self {
        let enabled = (0..catalog.rules.len())
            .map(crate::api::classification::RuleIndex::new)
            .collect();
        Self {
            catalog,
            enabled,
            limits: crate::AnalysisLimits::default(),
            artifact_cache: crate::analysis::ArtifactCacheHandle::default(),
        }
    }

    /// Select all rules at or above the requested confidence level.
    /// Construct a linter with rules at or above a confidence threshold.
    pub fn with_confidence(catalog: RuleCatalog, confidence: crate::api::rule::Confidence) -> Self {
        let enabled = catalog
            .rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                (rule.confidence() as u8 <= confidence as u8).then_some(index)
            })
            .map(crate::api::classification::RuleIndex::new)
            .collect();
        Self {
            catalog,
            enabled,
            limits: crate::AnalysisLimits::default(),
            artifact_cache: crate::analysis::ArtifactCacheHandle::default(),
        }
    }

    /// Construct a linter with a validated explicit rule selection.
    pub fn with_rules(
        catalog: RuleCatalog,
        enabled: impl IntoIterator<Item = RuleId>,
    ) -> Result<Self, LintConfigError> {
        let mut indices = enabled
            .into_iter()
            .map(|id| {
                catalog
                    .rule_index(&id)
                    .ok_or(LintConfigError::UnknownRule(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        indices.sort_unstable();
        indices.dedup();
        Ok(Self {
            catalog,
            enabled: indices,
            limits: crate::AnalysisLimits::default(),
            artifact_cache: crate::analysis::ArtifactCacheHandle::default(),
        })
    }

    /// Combine provider linters into one analysis pass under a shared host
    /// environment while preserving each linter's enabled rule selection.
    pub fn combine_with_environment(
        linters: impl IntoIterator<Item = Self>,
        environment: Environment,
    ) -> Result<Self, RuleCatalogError> {
        let mut catalogs = Vec::new();
        let mut enabled = BTreeSet::new();
        for linter in linters {
            enabled.extend(linter.enabled_rule_ids());
            catalogs.push(linter.catalog);
        }
        let catalog = RuleCatalog::combine_with_environment(catalogs, environment)?;
        Ok(Self::with_rules(catalog, enabled)
            .expect("combined catalog retains every selected rule"))
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

    /// Borrow the environment used by semantic analysis.
    pub fn analysis_environment(&self) -> &Environment {
        self.catalog.environment()
    }

    /// Borrow the validated parser and semantic safety limits.
    pub fn analysis_limits(&self) -> &crate::AnalysisLimits {
        &self.limits
    }

    pub(crate) fn artifact_cache_handle(&self) -> crate::analysis::ArtifactCacheHandle {
        self.artifact_cache.clone()
    }

    /// Lints an in-memory project using explicit, already-classified
    /// resolution results.  Filesystem loading belongs to the project crate.
    ///
    /// ```
    /// use glass_lint_core::{Linter, ProjectInput, RuleCatalog, SourceFile};
    ///
    /// let linter = Linter::new(RuleCatalog::new("example", vec![]).unwrap());
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
        let input = input.validate()?;
        tracing::info!(
            target: "glass_lint::project",
            files = input.sources.len(),
            resolutions = input.resolutions.len(),
            "project analysis started"
        );
        let mut session = self.begin_analysis(input.root)?;
        for source in input.sources {
            session.add_source(source)?;
        }
        for (key, result) in input.resolutions {
            session.record_resolution(key, result)?;
        }
        session.finish()
    }

    /// Analyze one in-memory source through the canonical project session.
    ///
    /// A snippet is a project containing one source. This convenience method
    /// returns the same source-free [`AnalysisReport`] shape as [`Self::lint`]
    /// and [`Self::lint_project`].
    ///
    /// ```
    /// use glass_lint_core::{Linter, RuleCatalog};
    ///
    /// let linter = Linter::new(RuleCatalog::new("example", vec![]).unwrap());
    /// let report = linter.lint_snippet("", "snippet.js").unwrap();
    /// assert_eq!(report.files[0].path.as_str(), "snippet.js");
    /// ```
    pub fn lint_snippet(
        &self,
        source: &str,
        filename: &str,
    ) -> Result<AnalysisReport, ProjectInputError> {
        let filename = crate::ProjectRelativePath::new(filename)?;
        let mut session = self.begin_analysis(".")?;
        session.add_source(crate::SourceFile::new(filename.to_string(), source)?)?;
        session.finish()
    }

    /// Finish the canonical project analysis and expose phase timings as
    /// observers of the same report-producing path.
    pub(crate) fn finish_analyzed_project(
        &self,
        input: ProjectInput,
        analyzed: AnalyzedModules,
        parse_diagnostics: BTreeMap<crate::ProjectRelativePath, crate::ParseDiagnostic>,
    ) -> Result<(AnalysisReport, std::time::Duration, std::time::Duration), ProjectInputError> {
        let input = input.validate()?;
        let ProjectFileState {
            sources,
            mut files,
            parse_paths,
        } = initialize_project_files(&input, parse_diagnostics);

        tracing::debug!(target: "glass_lint::project::link", modules = analyzed.len(), resolutions = input.resolutions.len(), "stage started");
        let linking_start = std::time::Instant::now();
        let project = ProjectSemanticModel::link_with_limits(input, analyzed, &self.limits)?;
        for (path, code) in parse_paths {
            project.record_parse_failure(path, &code);
        }
        let linking_elapsed = linking_start.elapsed();
        let link_counts = project.operation_counts(0);
        tracing::info!(target: "glass_lint::project::link", files = link_counts.files, requests = link_counts.requests, edges = link_counts.edges, elapsed = ?linking_elapsed, "stage finished");
        let matching_start = std::time::Instant::now();
        tracing::debug!(target: "glass_lint::project::matching", rules = self.enabled.len(), "stage started");
        let classifications = project.classify_with_evidence_limit(
            self.catalog.compiled(),
            &self.catalog.rules,
            &self.enabled,
            self.limits.evidence_items,
        );
        let matching_elapsed = matching_start.elapsed();
        self.populate_project_files(&project, &classifications, &sources, &mut files);

        let diagnostics = attach_project_diagnostics(&project, &mut files);
        let report = assemble_project_report(&project, files, diagnostics);

        let summary = report.summary();
        tracing::info!(target: "glass_lint::project::matching", files = report.operations.files, findings = summary.findings, evidence = report.operations.evidence, diagnostics = report.diagnostics.len() + summary.parse_diagnostics, elapsed = ?matching_elapsed, "stage finished");

        Ok((report, linking_elapsed, matching_elapsed))
    }

    fn populate_project_files(
        &self,
        project: &ProjectSemanticModel,
        classifications: &BTreeMap<ModuleId, ApiClassificationResult>,
        sources: &BTreeMap<crate::ProjectRelativePath, String>,
        files: &mut BTreeMap<crate::ProjectRelativePath, crate::FileReport>,
    ) {
        for module in project.modules() {
            let Some(classification) = classifications.get(&module.id()) else {
                continue;
            };
            let Some(source) = sources.get(module.path()) else {
                continue;
            };
            let mut findings =
                self.project_findings_for_module(project, module, classification, source);
            findings.sort_by_key(|finding| {
                (
                    finding.location.range.start().line(),
                    finding.location.range.start().column(),
                    finding.rule_id.clone(),
                )
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
        classification: &ApiClassificationResult,
        source: &str,
    ) -> Vec<crate::Finding> {
        self.findings_for(
            classification,
            &module.source().lines,
            source,
            module.path(),
        )
        .into_iter()
        .map(|finding| {
            let mut project_finding = finding;
            let finding_rule_id = project_finding.rule_id.clone();
            let related = classification
                .capabilities()
                .iter()
                .filter(|capability| {
                    self.catalog
                        .rule_id(capability.rule_index)
                        .is_some_and(|id| id == &finding_rule_id)
                })
                .flat_map(crate::api::classification::ApiCapability::evidence)
                .flat_map(|evidence| &evidence.related)
                .filter_map(|related| {
                    project
                        .fact_location(ModuleId::new(related.module), related.event)
                        .map(|mut location| {
                            location.message.clone_from(&related.symbol);
                            location
                        })
                });
            project_finding.append_related(related);
            project_finding
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Position, SourceRange,
        api::rule::{Confidence, Matcher, Rule, Severity},
        lint::{findings::contains_range, ranges::remove_contained_ranges},
    };
    fn catalog() -> RuleCatalog {
        let rule = Rule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        RuleCatalog::with_environment("test", vec![rule], environment).unwrap()
    }

    #[test]
    fn emits_one_located_finding_per_match() {
        let report = Linter::new(catalog()).lint("fetch('/a');\nfetch('/b');", "input.js");
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
        assert_eq!(report.files[0].findings[1].location.range.start().line(), 2);
        assert_eq!(report.files[0].findings[0].evidence.len(), 1);
        assert_eq!(report.files[0].findings[1].evidence.len(), 1);
        assert_eq!(
            report.files[0].findings[0].evidence[0].message,
            "call of \"fetch\""
        );
        assert_eq!(
            report.files[0].findings[0].evidence[0]
                .location
                .as_ref()
                .map(|location| &location.range),
            Some(&report.files[0].findings[0].location.range)
        );
        assert_eq!(
            report.files[0].findings[1].evidence[0]
                .location
                .as_ref()
                .map(|location| &location.range),
            Some(&report.files[0].findings[1].location.range)
        );
    }

    #[test]
    fn findings_only_carry_evidence_for_their_own_location() {
        let rule = Rule::builder("vault.write")
            .label("Writes vault files")
            .category("vault")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_call("app.vault.create"))
            .matcher(Matcher::rooted_member_call("app.vault.createFolder"))
            .build()
            .unwrap();
        let report = Linter::new(RuleCatalog::new("test", vec![rule]).unwrap()).lint(
            "this.app.vault.create('a');\nthis.app.vault.createFolder('b');",
            "input.js",
        );

        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(report.files[0].findings[0].evidence.len(), 1);
        assert_eq!(
            report.files[0].findings[0].evidence[0].message,
            "member_call of \"app.vault.create\""
        );
        assert_eq!(report.files[0].findings[1].evidence.len(), 1);
        assert_eq!(
            report.files[0].findings[1].evidence[0].message,
            "member_call of \"app.vault.createFolder\""
        );
    }

    #[test]
    fn rejects_shadowed_global_lookalikes() {
        let report = Linter::new(catalog()).lint(
            "function demo(fetch) { fetch('/local'); } fetch('/global');",
            "input.js",
        );
        assert_eq!(report.files[0].findings.len(), 1);
    }

    #[test]
    fn collapses_contained_ranges_for_same_rule() {
        let rule = Rule::builder("metadata.read")
            .label("Reads metadata")
            .category("metadata")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_read("app.metadataCache"))
            .matcher(Matcher::rooted_member_call(
                "app.metadataCache.getFileCache",
            ))
            .build()
            .unwrap();
        let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
        let report =
            Linter::new(catalog).lint("this.app.metadataCache.getFileCache(file);", "input.js");

        assert_eq!(report.files[0].findings.len(), 1);
        assert_eq!(
            report.files[0].findings[0].location.range.start().column(),
            1
        );
        assert_eq!(
            report.files[0].findings[0].location.range.end().column(),
            36
        );
        assert_eq!(report.files[0].findings[0].evidence.len(), 2);
        assert!(report.files[0].findings[0].evidence.iter().all(|evidence| {
            evidence.location.as_ref().is_some_and(|location| {
                contains_range(&report.files[0].findings[0].location.range, &location.range)
            })
        }));
    }

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
    fn validates_custom_rule_selection() {
        let unknown = RuleId::parse("test:missing").unwrap();
        assert!(matches!(
            Linter::with_rules(catalog(), [unknown]),
            Err(LintConfigError::UnknownRule(_))
        ));
    }

    #[test]
    fn reports_structured_diagnostic_for_oversized_source() {
        let report =
            Linter::new(catalog()).lint(&"x".repeat(crate::MAX_SOURCE_BYTES + 1), "large.js");
        assert!(report.files[0].findings.is_empty());
        assert_eq!(report.files[0].parse_diagnostic_count(), 1);
        assert_eq!(
            report.files[0].diagnostics[0]
                .parse_diagnostic()
                .unwrap()
                .code,
            crate::project::types::DiagnosticKind::SourceTooLarge.into()
        );
        assert_eq!(
            report.files[0].diagnostics[0]
                .parse_diagnostic()
                .unwrap()
                .filename,
            "large.js"
        );
        assert!(
            report.files[0].diagnostics[0]
                .parse_diagnostic()
                .unwrap()
                .range
                .is_none()
        );
    }

    #[test]
    fn parse_diagnostics_carry_stable_location_context() {
        let report = Linter::new(catalog()).lint("fetch(", "broken.js");
        assert!(report.files[0].findings.is_empty());
        let diagnostic = &report.files[0].diagnostics[0].parse_diagnostic().unwrap();
        assert_eq!(
            diagnostic.code,
            crate::project::types::DiagnosticKind::SyntaxError.into()
        );
        assert_eq!(diagnostic.filename, "broken.js");
        assert!(diagnostic.message.starts_with("JavaScript parse error:"));
        assert!(diagnostic.range.is_some());
    }

    #[test]
    fn source_locations_handle_crlf_and_eof_without_byte_columns() {
        let report = Linter::new(catalog()).lint("fetch('/a');\r\nfetch('/é');", "crlf.js");
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
        assert_eq!(report.files[0].findings[1].location.range.start().line(), 2);
        assert!(
            report.files[0].findings[1].location.range.end().column()
                > report.files[0].findings[1].location.range.start().column()
        );

        let empty = Linter::new(catalog()).lint("", "empty.js");
        assert!(empty.files[0].findings.is_empty());
        assert!(!empty.files[0].has_parse_diagnostics());
    }

    #[test]
    fn evidence_ranges_and_snippets_are_populated_for_unicode_source() {
        let report = Linter::new(catalog()).lint("// é\nfetch('/x');", "unicode.js");
        let evidence = &report.files[0].findings[0].evidence[0];
        assert_eq!(
            evidence
                .location
                .as_ref()
                .map(|location| location.range.start().line()),
            Some(2)
        );
    }

    #[test]
    fn evidence_limit_is_source_ordered_and_applied_once() {
        let source = (0..20).map(|_| "fetch();\n").collect::<String>();
        let report = Linter::new(catalog()).lint(&source, "many.js");
        assert_eq!(report.files[0].findings.len(), 20);
        assert_eq!(
            report.files[0]
                .findings
                .first()
                .unwrap()
                .location
                .range
                .start()
                .line(),
            1
        );
        assert_eq!(
            report.files[0]
                .findings
                .last()
                .unwrap()
                .location
                .range
                .start()
                .line(),
            20
        );
    }

    #[test]
    fn enabled_rule_order_does_not_affect_findings() {
        let rule_a = Rule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("XMLHttpRequest"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment
            .add_globals(["fetch", "XMLHttpRequest"])
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule_a, rule_b], environment).unwrap();

        let source = "fetch('/a'); new XMLHttpRequest();";
        let report_asc = Linter::with_rules(
            catalog.clone(),
            [
                RuleId::parse("test:alpha.first").unwrap(),
                RuleId::parse("test:beta.second").unwrap(),
            ],
        )
        .unwrap()
        .lint(source, "order.js");
        let report_desc = Linter::with_rules(
            catalog,
            [
                RuleId::parse("test:beta.second").unwrap(),
                RuleId::parse("test:alpha.first").unwrap(),
            ],
        )
        .unwrap()
        .lint(source, "order.js");

        // Both runs produce identical findings regardless of internal order.
        assert_eq!(
            report_asc.files[0].findings.len(),
            report_desc.files[0].findings.len()
        );
        for (a, b) in report_asc.files[0]
            .findings
            .iter()
            .zip(report_desc.files[0].findings.iter())
        {
            assert_eq!(a.rule_id, b.rule_id);
            assert_eq!(a.location.range, b.location.range);
            assert_eq!(a.message, b.message);
        }
    }

    #[test]
    fn disabled_catalog_rules_do_not_produce_findings() {
        let rule_a = Rule::builder("alpha.first")
            .label("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .label("Second")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("XMLHttpRequest"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment
            .add_globals(["fetch", "XMLHttpRequest"])
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule_a, rule_b], environment).unwrap();
        let report = Linter::with_rules(catalog, [RuleId::parse("test:beta.second").unwrap()])
            .unwrap()
            .lint("fetch(); XMLHttpRequest();", "subset.js");
        assert_eq!(report.files[0].findings.len(), 1);
        assert_eq!(
            report.files[0].findings[0].rule_id.as_str(),
            "test:beta.second"
        );
    }

    #[test]
    fn combines_provider_rules_with_overlapping_local_ids() {
        let first = Rule::builder("network.request")
            .label("First provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let second = Rule::builder("network.request")
            .label("Second provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();
        let linter = Linter::combine_with_environment(
            [
                Linter::new(RuleCatalog::new("first", vec![first]).unwrap()),
                Linter::new(RuleCatalog::new("second", vec![second]).unwrap()),
            ],
            environment,
        )
        .unwrap();

        let report = linter.lint("fetch('/a'); requestUrl('/b');", "combined.js");
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(
            report.files[0].findings[0].rule_id.as_str(),
            "first:network.request"
        );
        assert_eq!(
            report.files[0].findings[1].rule_id.as_str(),
            "second:network.request"
        );
    }

    #[test]
    fn combined_linter_preserves_each_input_rule_selection() {
        let enabled_rule = Rule::builder("enabled")
            .label("Enabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let disabled_rule = Rule::builder("disabled")
            .label("Disabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let enabled = Linter::new(RuleCatalog::new("first", vec![enabled_rule]).unwrap());
        let disabled =
            Linter::with_rules(RuleCatalog::new("second", vec![disabled_rule]).unwrap(), [])
                .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();

        let report = Linter::combine_with_environment([enabled, disabled], environment)
            .unwrap()
            .lint("fetch(); requestUrl();", "selection.js");

        assert_eq!(report.files[0].findings.len(), 1);
        assert_eq!(
            report.files[0].findings[0].rule_id.as_str(),
            "first:enabled"
        );
    }
}
