use std::{num::NonZeroUsize, sync::Arc};

use rayon::ThreadPoolBuilder;

use crate::{
    AnalysisLimits, Environment, ProjectAdmissionLimits, ProviderCatalogError, RuleId,
    analysis::ArtifactCacheHandle,
    api::classification::RuleIndex,
    lint::{
        batch::{BatchOptions, BatchResults, BatchStartError},
        catalog::RuleCatalog,
        selection::{LintConfigError, PreparedRuleSelection, RuleSelection},
    },
    project::{AnalysisReport, ProjectError, ProjectSession, SessionState},
};

/// Caller-supplied input to linter construction. Validation occurs in
/// [`Linter::new`].
#[derive(Clone, Debug)]
pub struct LinterConfig {
    rules: LinterRuleInputs,
    /// Host environment for global and global-object lookups.
    environment: Environment,
    /// Parser and semantic operation bounds.
    limits: AnalysisLimits,
    /// Aggregate source bounds for direct project sessions.
    project_limits: ProjectAdmissionLimits,
}

#[derive(Clone, Debug)]
enum LinterRuleInputs {
    Unprepared {
        /// Provider catalogs combined during linter construction.
        catalogs: Vec<RuleCatalog>,
        /// Baseline and per-rule overrides for the combined catalog.
        selection: RuleSelection,
    },
    Prepared(PreparedRuleSelection),
}

impl LinterConfig {
    pub fn new(catalogs: Vec<RuleCatalog>, environment: Environment) -> Self {
        Self {
            rules: LinterRuleInputs::Unprepared {
                catalogs,
                selection: RuleSelection::default(),
            },
            environment,
            limits: AnalysisLimits::default(),
            project_limits: ProjectAdmissionLimits::default(),
        }
    }

    #[must_use]
    pub fn with_rules(mut self, selection: RuleSelection) -> Self {
        let catalogs = match self.rules {
            LinterRuleInputs::Unprepared { catalogs, .. } => catalogs,
            LinterRuleInputs::Prepared(prepared) => {
                let (catalog, _) = prepared.into_parts();
                vec![catalog]
            }
        };
        self.rules = LinterRuleInputs::Unprepared {
            catalogs,
            selection,
        };
        self
    }

    /// Use a catalog-bound selection prepared by [`RuleSelection::prepare`].
    /// The prepared catalog and indexes become the sole rule configuration.
    #[must_use]
    pub fn with_prepared_rules(mut self, selection: PreparedRuleSelection) -> Self {
        self.rules = LinterRuleInputs::Prepared(selection);
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: AnalysisLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_project_limits(mut self, limits: ProjectAdmissionLimits) -> Self {
        self.project_limits = limits;
        self
    }

    pub fn selection(&self) -> &RuleSelection {
        match &self.rules {
            LinterRuleInputs::Unprepared { selection, .. } => selection,
            LinterRuleInputs::Prepared(selection) => selection.selection(),
        }
    }
}

/// Immutable configuration shared across cloned linters.
#[derive(Clone)]
struct LinterSharedConfig {
    /// Validated rule catalog and compiled matcher plans.
    catalog: RuleCatalog,
    /// Host environment used during semantic fact construction.
    environment: Environment,
    /// Enabled rule indexes in deterministic catalog order.
    enabled: Vec<RuleIndex>,
    /// Parser and semantic operation bounds.
    limits: AnalysisLimits,
    /// Aggregate source bounds for direct project sessions.
    project_limits: ProjectAdmissionLimits,
}

/// Immutable catalog plus sorted enabled-rule indexes for lint execution.
///
/// The linter owns the combined rule catalog, host environment, enabled-rule
/// set, analysis limits, and a shared bounded artifact cache. It is `Send`
/// and `Sync` and can be cloned cheaply (all configuration fields are
/// `Arc`-backed; only the already-shared cache handle is cloned separately).
pub struct Linter {
    /// Arc-backed immutable configuration shared between clones.
    shared: Arc<LinterSharedConfig>,
    /// Shared bounded cache of successfully analyzed artifacts.
    artifact_cache: ArtifactCacheHandle,
}

impl Clone for Linter {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}

impl Linter {
    /// Starts a deterministic project collection session.
    pub fn begin_project(&self) -> ProjectSession<'_> {
        let state = SessionState::new(
            self.analysis_environment(),
            self.analysis_limits(),
            self.artifact_cache_handle(),
            &self.shared.catalog,
            &self.shared.enabled,
            self.shared.limits.evidence_items(),
            self.shared.project_limits,
        );
        ProjectSession::new(state)
    }

    /// Construct a linter from validated catalogs, environment, rule
    /// selection, and analysis limits. Catalogs are combined into one
    /// unified catalog (rejecting duplicate fully-qualified IDs), rule
    /// overrides are applied in declaration order, and limits are validated.
    pub fn new(config: LinterConfig) -> Result<Self, LintConfigError> {
        let (catalog, enabled) = match config.rules {
            LinterRuleInputs::Prepared(prepared) => prepared.into_parts(),
            LinterRuleInputs::Unprepared {
                catalogs,
                selection,
            } => {
                let catalog = RuleCatalog::combine(catalogs).map_err(|error| match error {
                    ProviderCatalogError::InvalidRule(id, diagnostic) => {
                        LintConfigError::InvalidRule(id, diagnostic)
                    }
                    ProviderCatalogError::DuplicateRule(id) => LintConfigError::DuplicateRule(id),
                    ProviderCatalogError::InvalidRuleId(id) => LintConfigError::InvalidSelector(id),
                })?;
                let enabled = selection.resolve(&catalog)?;
                (catalog, enabled)
            }
        };

        // Limits are guaranteed valid by construction through
        // `AnalysisLimits::default` or its named builders; no re-validation
        // is needed.
        Ok(Self {
            shared: Arc::new(LinterSharedConfig {
                catalog,
                environment: config.environment,
                enabled,
                limits: config.limits,
                project_limits: config.project_limits,
            }),
            artifact_cache: ArtifactCacheHandle::default(),
        })
    }

    #[must_use]
    /// Borrow the validated catalog.
    pub fn catalog(&self) -> &RuleCatalog {
        &self.shared.catalog
    }

    /// Returns the enabled rule IDs in deterministic catalog order.
    #[must_use]
    pub fn enabled_rule_ids(&self) -> Vec<RuleId> {
        self.shared
            .enabled
            .iter()
            .filter_map(|&index| self.shared.catalog.rule_id(index).cloned())
            .collect()
    }

    /// Borrow the validated parser and semantic safety limits.
    pub fn analysis_limits(&self) -> &AnalysisLimits {
        &self.shared.limits
    }

    /// Borrow the complete host environment used by semantic analysis.
    pub fn analysis_environment(&self) -> &Environment {
        &self.shared.environment
    }

    pub(crate) fn artifact_cache_handle(&self) -> ArtifactCacheHandle {
        self.artifact_cache.clone()
    }

    /// Analyze one owned source through the canonical project session.
    ///
    /// ```
    /// use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog};
    ///
    /// let linter = Linter::new(LinterConfig::new(
    ///     vec![RuleCatalog::new("example", vec![]).unwrap()],
    ///     Environment::default(),
    /// ))
    /// .unwrap();
    /// let source = glass_lint_core::project::SourceFile::new("snippet.js", "").unwrap();
    /// let report = linter.lint_source(source).unwrap();
    /// assert_eq!(report.files()[0].path().as_str(), "snippet.js");
    /// ```
    pub fn lint_source(
        &self,
        source: crate::project::SourceFile,
    ) -> Result<AnalysisReport, ProjectError> {
        self.run_single_source(source)
    }

    pub(crate) fn run_single_source(
        &self,
        source: crate::project::SourceFile,
    ) -> Result<AnalysisReport, ProjectError> {
        let mut collection = self.begin_project();
        collection.analyze_source(source)?;
        Ok(collection.finish([])?.into_report())
    }

    /// Lint independent owned sources in a bounded, input-ordered stream.
    pub fn lint_batch<I>(
        &self,
        sources: I,
        options: BatchOptions,
    ) -> Result<BatchResults<I::IntoIter>, BatchStartError>
    where
        I: IntoIterator<Item = crate::project::SourceFile>,
    {
        let available = std::thread::available_parallelism().map_or(usize::MAX, NonZeroUsize::get);
        let worker_count = options
            .workers()
            .get()
            .min(available)
            .min(options.max_in_flight().get())
            .max(1);
        let pool = ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(|_| BatchStartError::WorkerPoolUnavailable)?;
        let channel = std::sync::mpsc::channel();
        let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        Ok(BatchResults::new(
            sources.into_iter(),
            self.clone(),
            pool,
            channel,
            cancellation,
            options.max_in_flight(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{Position, SourceRange};

    use crate::{
        Environment, LintConfigError, Linter, LinterConfig, RuleBaseline, RuleCatalog,
        RuleOverride, RuleSelection, RuleState,
        lint::ranges::remove_contained_ranges,
        rules::{Confidence, EventQuery, Rule, Severity},
    };

    #[test]
    fn remove_contained_ranges_keeps_only_largest() {
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
    fn findings_are_sorted_by_position() {
        let rule = Rule::catalog_builder("network.request")
            .description("Uses fetch")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .query(EventQuery::call_global("fetch"))
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
            .lint_source(
                crate::project::SourceFile::new("sort.js", "fetch('/b'); fetch('/a');").unwrap(),
            )
            .unwrap();
        // Findings should be sorted by line, then column, then rule ID.
        assert_eq!(report.files()[0].findings().len(), 2);
        assert_eq!(
            report.files()[0].findings()[0]
                .location()
                .range()
                .start()
                .line(),
            1
        );
        assert_eq!(
            report.files()[0].findings()[0]
                .location()
                .range()
                .start()
                .column(),
            1
        );
        assert_eq!(
            report.files()[0].findings()[1]
                .location()
                .range()
                .start()
                .column(),
            14
        );
    }

    #[test]
    fn classify_groups_findings_by_rule() {
        let rule = Rule::catalog_builder("network.request")
            .description("Uses fetch")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .query(EventQuery::call_global("fetch"))
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
            .lint_source(
                crate::project::SourceFile::new("classify.js", "fetch('/a'); fetch('/b');")
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(report.files()[0].findings().len(), 2);
        assert_eq!(
            report.files()[0].findings()[0].rule_id().as_str(),
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
