use std::{num::NonZeroUsize, path::PathBuf};

use anyhow::{Result, bail};
use glass_lint_core::AnalysisLimits;
use glass_lint_project::ValidatedProjectLoadOptions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileCatalogProvider {
    Js,
    Obsidian,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleSelectionProfile {
    Recommended,
    Heuristic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileWorkload {
    Files,
    LoaderProject,
    AdmittedProject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileCorpusIdentity {
    Verified(String),
    Unverified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileWorkloadIdentity {
    pub mode: ProfileWorkload,
    pub corpus: ProfileCorpusIdentity,
    pub execution: ProfileExecutionIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileExecutionIdentity {
    pub provider: ProfileCatalogProvider,
    pub mode: RuleSelectionProfile,
    pub rules: Vec<String>,
    pub workers: usize,
    pub continue_on_error: bool,
    pub sample: Option<usize>,
    pub seed: u64,
    pub analysis_limits: ProfileAnalysisLimits,
    pub project_load: ProfileProjectLoadIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAnalysisLimits {
    pub syntax_depth: usize,
    pub semantic_operations: usize,
    pub effect_operations: usize,
    pub evidence_items: usize,
    pub link_operations: usize,
    pub flow_operations: usize,
    pub trace_nodes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileProjectLoadIdentity {
    pub max_files: usize,
    pub max_requests: usize,
    pub max_source_bytes: u64,
    pub max_project_source_bytes: u64,
    pub max_visited_entries: usize,
    pub max_timeout_ms: u64,
    pub max_config_count: usize,
    pub max_config_depth: usize,
    pub extensions: Vec<String>,
    pub extension_aliases: Vec<(String, Vec<String>)>,
    pub follow_symlinks: bool,
}

/// Validated-by-`run_profile` controls for one profile run.
#[derive(Clone, Debug)]
pub struct ProfileConfig {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) include: Vec<String>,
    pub(crate) exclude: Vec<String>,
    pub(crate) sample: Option<usize>,
    pub(crate) seed: u64,
    pub(crate) warm_up: usize,
    pub(crate) repeat: NonZeroUsize,
    pub(crate) continue_on_error: bool,
    pub(crate) workers: NonZeroUsize,
    pub(crate) provider: ProfileCatalogProvider,
    pub(crate) mode: RuleSelectionProfile,
    pub(crate) rules: Vec<String>,
    pub(crate) workload: ProfileWorkload,
    pub(crate) manifest: Option<PathBuf>,
}

/// Validated public construction path for profile runs.
#[derive(Clone, Debug)]
pub struct ProfileConfigBuilder {
    config: ProfileConfig,
}

impl ProfileConfig {
    pub fn builder(paths: impl IntoIterator<Item = PathBuf>) -> ProfileConfigBuilder {
        ProfileConfigBuilder {
            config: Self {
                paths: paths.into_iter().collect(),
                include: Vec::new(),
                exclude: Vec::new(),
                sample: None,
                seed: 0,
                warm_up: 0,
                repeat: NonZeroUsize::MIN,
                continue_on_error: false,
                workers: NonZeroUsize::MIN,
                provider: ProfileCatalogProvider::Js,
                mode: RuleSelectionProfile::Recommended,
                rules: Vec::new(),
                workload: ProfileWorkload::Files,
                manifest: None,
            },
        }
    }

    pub(crate) fn execution_identity(&self) -> ProfileExecutionIdentity {
        let limits = AnalysisLimits::default();
        let load = ValidatedProjectLoadOptions::default();
        let mut rules = self.rules.clone();
        rules.sort();
        ProfileExecutionIdentity {
            provider: self.provider,
            mode: self.mode,
            rules,
            workers: self.workers.get(),
            continue_on_error: self.continue_on_error,
            sample: self.sample,
            seed: self.seed,
            analysis_limits: ProfileAnalysisLimits {
                syntax_depth: limits.syntax_depth(),
                semantic_operations: limits.semantic_operations(),
                effect_operations: limits.effect_operations(),
                evidence_items: limits.evidence_items(),
                link_operations: limits.link_operations(),
                flow_operations: limits.flow_operations(),
                trace_nodes: limits.trace_nodes(),
            },
            project_load: ProfileProjectLoadIdentity {
                max_files: load.max_files(),
                max_requests: load.max_requests(),
                max_source_bytes: load.max_source_bytes(),
                max_project_source_bytes: load.max_project_source_bytes(),
                max_visited_entries: load.max_visited_entries(),
                max_timeout_ms: load.max_timeout_ms(),
                max_config_count: load.max_config_count(),
                max_config_depth: load.max_config_depth(),
                extensions: load.extensions().map(str::to_owned).collect(),
                extension_aliases: load
                    .extension_aliases()
                    .map(|(extension, aliases)| (extension.to_owned(), aliases.to_vec()))
                    .collect(),
                follow_symlinks: load.follow_symlinks(),
            },
        }
    }
}

impl ProfileConfigBuilder {
    #[must_use]
    pub fn include(mut self, values: impl IntoIterator<Item = String>) -> Self {
        self.config.include = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn exclude(mut self, values: impl IntoIterator<Item = String>) -> Self {
        self.config.exclude = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn sample(mut self, value: Option<usize>) -> Self {
        self.config.sample = value;
        self
    }

    #[must_use]
    pub fn seed(mut self, value: u64) -> Self {
        self.config.seed = value;
        self
    }

    #[must_use]
    pub fn warm_up(mut self, value: usize) -> Self {
        self.config.warm_up = value;
        self
    }

    #[must_use]
    pub fn repeat(mut self, value: NonZeroUsize) -> Self {
        self.config.repeat = value;
        self
    }

    #[must_use]
    pub fn workers(mut self, value: NonZeroUsize) -> Self {
        self.config.workers = value;
        self
    }

    #[must_use]
    pub fn continue_on_error(mut self, value: bool) -> Self {
        self.config.continue_on_error = value;
        self
    }

    #[must_use]
    pub fn provider(mut self, value: ProfileCatalogProvider) -> Self {
        self.config.provider = value;
        self
    }

    #[must_use]
    pub fn mode(mut self, value: RuleSelectionProfile) -> Self {
        self.config.mode = value;
        self
    }

    #[must_use]
    pub fn rules(mut self, value: impl IntoIterator<Item = String>) -> Self {
        self.config.rules = value.into_iter().collect();
        self
    }

    #[must_use]
    pub fn workload(mut self, value: ProfileWorkload) -> Self {
        self.config.workload = value;
        self
    }

    #[must_use]
    pub fn manifest(mut self, value: Option<PathBuf>) -> Self {
        self.config.manifest = value;
        self
    }

    pub fn build(self) -> Result<ProfileConfig> {
        validate_config(&self.config)?;
        Ok(self.config)
    }
}

pub(super) fn validate_config(config: &ProfileConfig) -> Result<()> {
    if config.paths.is_empty() {
        bail!("at least one --path is required");
    }
    if config.sample == Some(0) {
        bail!("--sample must be at least 1");
    }
    if matches!(config.workload, ProfileWorkload::AdmittedProject) && config.paths.len() != 1 {
        bail!("--admitted-project requires exactly one --path root");
    }
    if config.manifest.is_some() && config.paths.len() != 1 {
        bail!("--manifest requires exactly one --path root");
    }
    if matches!(config.workload, ProfileWorkload::AdmittedProject) && !config.paths[0].is_dir() {
        bail!("--admitted-project root must be a directory");
    }
    Ok(())
}
