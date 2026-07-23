use std::sync::Arc;

use crate::{
    AnalysisLimits, Environment, ProviderCatalogError, RuleId,
    analysis::ArtifactCacheHandle,
    api::classification::RuleIndex,
    lint::{
        catalog::RuleCatalog,
        selection::{LintConfigError, RuleBaseline, RuleSelection, RuleState},
    },
    project::{AnalysisReport, ProjectCollection, ProjectInputError, SessionState},
};

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
    limits: AnalysisLimits,
}

impl LinterConfig {
    pub fn new(catalogs: Vec<RuleCatalog>, environment: Environment) -> Self {
        Self {
            catalogs,
            environment,
            selection: RuleSelection::default(),
            limits: AnalysisLimits::default(),
        }
    }

    #[must_use]
    pub fn with_rules(mut self, selection: RuleSelection) -> Self {
        self.selection = selection;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: AnalysisLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn selection(&self) -> &RuleSelection {
        &self.selection
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
    /// Shared bounded cache of successfully lowered artifacts.
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
    pub fn begin_project(
        &self,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<ProjectCollection<'_>, ProjectInputError> {
        let state = SessionState::new(
            self.analysis_environment(),
            self.analysis_limits(),
            self.artifact_cache_handle(),
            &self.shared.catalog,
            &self.shared.enabled,
            self.shared.limits.evidence_items(),
        );
        ProjectCollection::new(state, root)
    }

    /// Construct a linter from validated catalogs, environment, rule
    /// selection, and analysis limits. Catalogs are combined into one
    /// unified catalog (rejecting duplicate fully-qualified IDs), rule
    /// overrides are applied in declaration order, and limits are validated.
    pub fn new(config: LinterConfig) -> Result<Self, LintConfigError> {
        let catalog = RuleCatalog::combine(config.catalogs).map_err(|error| match error {
            ProviderCatalogError::InvalidRule(id, _) => LintConfigError::DuplicateRule(id),
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
                enabled.push(RuleIndex::new(index));
            }
        }
        // Limits are guaranteed valid by construction through
        // `AnalysisLimits::new` or `Default`; no re-validation needed.
        Ok(Self {
            shared: Arc::new(LinterSharedConfig {
                catalog,
                environment: config.environment,
                enabled,
                limits: config.limits,
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

    /// Analyze one in-memory source through the canonical project session.
    ///
    /// A snippet is a project containing one source. This convenience method
    /// returns the same source-free [`AnalysisReport`] shape as the full
    /// staged session pipeline.
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
        let filename = crate::project::ProjectRelativePath::new(filename)?;
        let mut collection = self.begin_project(".")?;
        collection.analyze_source(crate::project::SourceFile::new(
            filename.to_string(),
            source,
        )?)?;
        collection.finish_local().resolve([])?.finish()
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
