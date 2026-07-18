use std::collections::BTreeMap;

use super::catalog::RuleCatalog;
use crate::{
    AnalysisReport, AnalysisSession, Environment, ProjectInput, ProjectInputError,
    ProviderCatalogError, REPORT_VERSION, RuleId,
    analysis::{LocalArtifact, ProjectSemanticModel},
    api::classification::ClassificationResult,
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
    /// A selector is malformed or did not select any assembled rule.
    InvalidSelector(String),
    /// A catalog contains the same fully-qualified rule more than once.
    DuplicateRule(RuleId),
    /// Safety limits are invalid.
    InvalidLimits(String),
}

impl std::fmt::Display for LintConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(f, "unknown rule `{id}`"),
            Self::InvalidSelector(message) => write!(f, "invalid rule selector: {message}"),
            Self::DuplicateRule(id) => write!(f, "duplicate rule `{id}`"),
            Self::InvalidLimits(message) => write!(f, "invalid resource limits: {message}"),
        }
    }
}

impl std::error::Error for LintConfigError {}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleBaseline {
    #[default]
    All,
    None,
    MinimumConfidence(crate::api::rule::Confidence),
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuleState {
    Disabled,
    Enabled,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleOverride {
    #[serde(deserialize_with = "deserialize_selector")]
    selector: RuleSelector,
    enabled: bool,
}

/// Parsed rule selector. The wildcard language is intentionally tiny: `*`
/// matches any sequence of characters, while all other characters are
/// literal. Keeping the parsed shape here prevents validation and execution
/// from maintaining separate interpretations of the same selector.
#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct RuleSelector(String);

fn deserialize_selector<'de, D>(deserializer: D) -> Result<RuleSelector, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    RuleSelector::parse(value).map_err(serde::de::Error::custom)
}

impl RuleSelector {
    fn parse(selector: String) -> Result<Self, LintConfigError> {
        if selector.is_empty()
            || selector
                .chars()
                .any(|c| c == '?' || c == '[' || c == ']' || c == '{' || c == '}')
        {
            return Err(LintConfigError::InvalidSelector(selector));
        }
        RuleId::parse(selector.replace('*', "placeholder"))
            .map_err(|_| LintConfigError::InvalidSelector(selector.clone()))?;
        Ok(Self(selector))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn matches(&self, id: &str) -> bool {
        self.0.split('*').enumerate().all(|(index, part)| {
            if index == 0 {
                id.starts_with(part)
            } else {
                id.contains(part)
            }
        }) && (self.0.ends_with('*') || id.ends_with(self.0.rsplit('*').next().unwrap_or_default()))
    }
}

impl RuleOverride {
    pub fn new(selector: impl Into<String>, state: RuleState) -> Result<Self, LintConfigError> {
        let selector = RuleSelector::parse(selector.into())?;
        Ok(Self {
            selector,
            enabled: state == RuleState::Enabled,
        })
    }

    pub fn selector(&self) -> &str {
        self.selector.as_str()
    }

    pub fn state(&self) -> RuleState {
        if self.enabled {
            RuleState::Enabled
        } else {
            RuleState::Disabled
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleSelection {
    baseline: RuleBaseline,
    overrides: Vec<RuleOverride>,
}

impl Default for RuleSelection {
    fn default() -> Self {
        Self::new(RuleBaseline::All)
    }
}

impl RuleSelection {
    pub fn new(baseline: RuleBaseline) -> Self {
        Self {
            baseline,
            overrides: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_override(mut self, value: RuleOverride) -> Self {
        self.overrides.push(value);
        self
    }

    pub fn baseline(&self) -> RuleBaseline {
        self.baseline
    }

    pub fn overrides(&self) -> &[RuleOverride] {
        &self.overrides
    }
}

pub fn validate_selection(
    selection: &RuleSelection,
    catalog: &RuleCatalog,
) -> Result<(), LintConfigError> {
    for override_ in selection.overrides() {
        if catalog
            .rule_ids()
            .iter()
            .any(|id| override_.selector.matches(id.as_str()))
        {
            continue;
        }
        if !override_.selector.as_str().contains('*') {
            return Err(LintConfigError::UnknownRule(
                RuleId::parse(override_.selector.as_str().to_owned()).map_err(|_| {
                    LintConfigError::InvalidSelector(override_.selector.as_str().into())
                })?,
            ));
        }
        return Err(LintConfigError::InvalidSelector(
            override_.selector.as_str().into(),
        ));
    }
    Ok(())
}

/// Caller-supplied input to linter construction. Validation occurs in
/// [`Linter::new`].
#[derive(Clone, Debug)]
pub struct LinterConfig {
    catalogs: Vec<RuleCatalog>,
    environment: Environment,
    selection: RuleSelection,
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
pub struct Linter {
    /// Validated rule catalog and compiled matcher plans.
    catalog: RuleCatalog,
    environment: Environment,
    /// Enabled rule indexes in deterministic order.
    enabled: Vec<crate::api::classification::RuleIndex>,
    limits: crate::AnalysisLimits,
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

    /// Construct a linter from validated catalogs, one complete environment,
    /// rule selection, and analysis limits.
    pub fn new(config: LinterConfig) -> Result<Self, LintConfigError> {
        let catalog = RuleCatalog::combine(config.catalogs).map_err(|error| match error {
            ProviderCatalogError::InvalidRule(id, _) => {
                LintConfigError::DuplicateRule(RuleId::parse(id).expect("catalog IDs validated"))
            }
            ProviderCatalogError::InvalidRuleId(id) => LintConfigError::InvalidSelector(id),
        })?;
        validate_selection(&config.selection, &catalog)?;
        let mut enabled = Vec::new();
        for (index, rule_id) in catalog.rule_ids().iter().enumerate() {
            let baseline = match config.selection.baseline {
                RuleBaseline::All => true,
                RuleBaseline::None => false,
                RuleBaseline::MinimumConfidence(confidence) => {
                    catalog.rules[index].confidence() as u8 <= confidence as u8
                }
            };
            let mut state = baseline;
            for override_ in &config.selection.overrides {
                if override_.selector.matches(rule_id.as_str()) {
                    state = override_.state() == RuleState::Enabled;
                }
            }
            if state {
                enabled.push(crate::api::classification::RuleIndex::new(index));
            }
        }
        config
            .limits
            .validate()
            .map_err(LintConfigError::InvalidLimits)?;
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
        classifications: &BTreeMap<ModuleId, ClassificationResult>,
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
        classification: &ClassificationResult,
        source: &str,
    ) -> Vec<crate::Finding> {
        self.findings_for(
            classification,
            &module.source_context().lines,
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
                .flat_map(crate::api::classification::MatchedCapability::evidence)
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
            .description("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        RuleCatalog::new("test", vec![rule]).unwrap()
    }

    fn test_linter(catalog: RuleCatalog, environment: crate::Environment) -> Linter {
        Linter::new(LinterConfig::new(vec![catalog], environment)).unwrap()
    }

    fn catalog_linter(catalog: RuleCatalog) -> Linter {
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        test_linter(catalog, environment)
    }

    #[test]
    fn emits_one_located_finding_per_match() {
        let report = catalog_linter(catalog()).lint("fetch('/a');\nfetch('/b');", "input.js");
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
            .description("Writes vault files")
            .category("vault")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(Matcher::rooted_member_call("app.vault.create"))
            .matcher(Matcher::rooted_member_call("app.vault.createFolder"))
            .build()
            .unwrap();
        let report = test_linter(
            RuleCatalog::new("test", vec![rule]).unwrap(),
            crate::Environment::default(),
        )
        .lint(
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
        let report = catalog_linter(catalog()).lint(
            "function demo(fetch) { fetch('/local'); } fetch('/global');",
            "input.js",
        );
        assert_eq!(report.files[0].findings.len(), 1);
    }

    #[test]
    fn collapses_contained_ranges_for_same_rule() {
        let rule = Rule::builder("metadata.read")
            .description("Reads metadata")
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
        let report = test_linter(catalog, crate::Environment::default())
            .lint("this.app.metadataCache.getFileCache(file);", "input.js");

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
            Linter::new(
                LinterConfig::new(vec![catalog()], Environment::default()).with_rules(
                    RuleSelection::new(RuleBaseline::None).with_override(
                        RuleOverride::new(unknown.to_string(), RuleState::Enabled).unwrap(),
                    ),
                ),
            ),
            Err(LintConfigError::UnknownRule(_))
        ));
    }

    #[test]
    fn ordered_rule_overrides_select_stable_catalog_indexes() {
        let first = Rule::builder("network.first")
            .description("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let second = Rule::builder("network.second")
            .description("Second")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let catalog = RuleCatalog::new("test", vec![first, second]).unwrap();
        let selection = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("test:*", RuleState::Enabled).unwrap())
            .with_override(RuleOverride::new("test:network.first", RuleState::Disabled).unwrap());
        let linter = Linter::new(
            LinterConfig::new(vec![catalog], Environment::default()).with_rules(selection),
        )
        .unwrap();
        assert_eq!(
            linter.enabled_rule_ids(),
            vec![RuleId::parse("test:network.second").unwrap()]
        );
    }

    #[test]
    fn selectors_require_a_known_match() {
        let catalog = RuleCatalog::new("test", vec![]).unwrap();
        let selection = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("test:missing", RuleState::Enabled).unwrap());
        assert!(matches!(
            Linter::new(
                LinterConfig::new(vec![catalog], Environment::default()).with_rules(selection)
            ),
            Err(LintConfigError::UnknownRule(_))
        ));
    }

    #[test]
    fn reports_structured_diagnostic_for_oversized_source() {
        let report =
            catalog_linter(catalog()).lint(&"x".repeat(crate::MAX_SOURCE_BYTES + 1), "large.js");
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
        let report = catalog_linter(catalog()).lint("fetch(", "broken.js");
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
        let report = catalog_linter(catalog()).lint("fetch('/a');\r\nfetch('/é');", "crlf.js");
        assert_eq!(report.files[0].findings.len(), 2);
        assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
        assert_eq!(report.files[0].findings[1].location.range.start().line(), 2);
        assert!(
            report.files[0].findings[1].location.range.end().column()
                > report.files[0].findings[1].location.range.start().column()
        );

        let empty = catalog_linter(catalog()).lint("", "empty.js");
        assert!(empty.files[0].findings.is_empty());
        assert!(!empty.files[0].has_parse_diagnostics());
    }

    #[test]
    fn evidence_ranges_and_snippets_are_populated_for_unicode_source() {
        let report = catalog_linter(catalog()).lint("// é\nfetch('/x');", "unicode.js");
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
        let report = catalog_linter(catalog()).lint(&source, "many.js");
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
            .description("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .description("Second")
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
        let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();

        let source = "fetch('/a'); new XMLHttpRequest();";
        let enabled = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("test:alpha.first", RuleState::Enabled).unwrap())
            .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap());
        let report_asc = Linter::new(
            LinterConfig::new(vec![catalog.clone()], environment.clone()).with_rules(enabled),
        )
        .unwrap()
        .lint(source, "order.js");
        let enabled = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap())
            .with_override(RuleOverride::new("test:alpha.first", RuleState::Enabled).unwrap());
        let report_desc =
            Linter::new(LinterConfig::new(vec![catalog], environment).with_rules(enabled))
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
            .description("First")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let rule_b = Rule::builder("beta.second")
            .description("Second")
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
        let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();
        let selection = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap());
        let report =
            Linter::new(LinterConfig::new(vec![catalog], environment).with_rules(selection))
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
            .description("First provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let second = Rule::builder("network.request")
            .description("Second provider request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();
        let linter = Linter::new(LinterConfig::new(
            vec![
                RuleCatalog::new("first", vec![first]).unwrap(),
                RuleCatalog::new("second", vec![second]).unwrap(),
            ],
            environment,
        ))
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
            .description("Enabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let disabled_rule = Rule::builder("disabled")
            .description("Disabled")
            .category("test")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("requestUrl"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_globals(["fetch", "requestUrl"]).unwrap();
        let selection = RuleSelection::new(RuleBaseline::None)
            .with_override(RuleOverride::new("first:enabled", RuleState::Enabled).unwrap());
        let report = Linter::new(
            LinterConfig::new(
                vec![
                    RuleCatalog::new("first", vec![enabled_rule]).unwrap(),
                    RuleCatalog::new("second", vec![disabled_rule]).unwrap(),
                ],
                environment,
            )
            .with_rules(selection),
        )
        .unwrap()
        .lint("fetch(); requestUrl();", "selection.js");

        assert_eq!(report.files[0].findings.len(), 1);
        assert_eq!(
            report.files[0].findings[0].rule_id.as_str(),
            "first:enabled"
        );
    }
}
