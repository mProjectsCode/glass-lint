//! Configuration schema, loading precedence, and provider/profile selection.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use glass_lint_core::{
    CoreConfig, Linter, MAX_SOURCE_BYTES, RuleBaseline, RuleCatalog, RuleSelection, Severity,
};
use glass_lint_project::{ProjectLoadOptions, ValidatedProjectLoadOptions};
use serde::{Deserialize, Serialize};

use crate::args::Args;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Obsidian rules together with the generic JavaScript catalog.
    #[default]
    Obsidian,
    Js,
    Node,
    Electron,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RuleSelectionProfile {
    /// Only high-confidence rules selected for normal use.
    Recommended,
    #[default]
    Heuristic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    /// Exit unsuccessfully for any finding, including informational findings.
    Info,
    Warning,
    #[default]
    Error,
    Never,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable diagnostics and a deterministic summary.
    #[default]
    Pretty,
    Json,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    /// Emit no telemetry beyond errors written by the command.
    #[default]
    Quiet,
    Normal,
    Verbose,
    Trace,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CliConfig {
    #[serde(default)]
    /// Rule provider whose catalog and host environment are enabled.
    pub provider: Provider,
    #[serde(default)]
    /// Rule confidence profile used to select the catalog.
    pub profile: RuleSelectionProfile,
    #[serde(default)]
    /// Filesystem and project-loading limits, owned by the project boundary.
    pub project: ProjectConfig,
    #[serde(default)]
    /// Minimum finding severity that makes the command fail.
    pub fail_on: FailOn,
    #[serde(default)]
    /// Serialization format for findings and summaries.
    pub output: OutputFormat,
    #[serde(default)]
    /// Amount of telemetry emitted while the command runs.
    pub verbosity: Verbosity,
    #[serde(default = "default_color")]
    /// Whether supported human-readable output may use terminal colors.
    pub color: bool,
    #[serde(default = "default_width")]
    /// Maximum line width used by pretty output before it wraps evidence.
    pub pretty_max_width: usize,
    #[serde(default = "default_show_evidence_source")]
    /// Whether pretty output includes source excerpts for evidence rows.
    pub show_evidence_source: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct ProjectConfig {
    #[serde(default = "default_max_bytes")]
    pub max_source_bytes: u64,
    #[serde(default = "default_project_bytes")]
    pub max_project_source_bytes: u64,
    #[serde(default = "default_visited_entries")]
    pub max_visited_entries: usize,
    #[serde(default = "default_timeout_ms")]
    pub max_timeout_ms: u64,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            max_source_bytes: default_max_bytes(),
            max_project_source_bytes: default_project_bytes(),
            max_visited_entries: default_visited_entries(),
            max_timeout_ms: default_timeout_ms(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Versioned top-level configuration consumed by the CLI.
    pub version: u32,
    #[serde(default)]
    /// Provider-neutral analysis limits and matcher configuration.
    pub core: CoreConfig,
    #[serde(default)]
    /// CLI provider, profile, output, and exit-policy settings.
    pub cli: CliConfig,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            provider: Provider::default(),
            profile: RuleSelectionProfile::default(),
            project: ProjectConfig::default(),
            fail_on: FailOn::default(),
            output: OutputFormat::default(),
            verbosity: Verbosity::default(),
            color: default_color(),
            pretty_max_width: default_width(),
            show_evidence_source: default_show_evidence_source(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 2,
            core: CoreConfig::default(),
            cli: CliConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    version: Option<u32>,
    #[serde(default)]
    core: CoreConfig,
    #[serde(default)]
    cli: CliConfig,
}

fn default_max_bytes() -> u64 {
    MAX_SOURCE_BYTES as u64
}

fn default_width() -> usize {
    160
}

fn default_project_bytes() -> u64 {
    512 * 1024 * 1024
}
fn default_visited_entries() -> usize {
    250_000
}

fn default_timeout_ms() -> u64 {
    5 * 60 * 1000
}

fn default_color() -> bool {
    true
}

fn default_show_evidence_source() -> bool {
    true
}

/// Resolve configuration from inline JSON, an explicit file, or the cwd.
///
/// Inline JSON has highest precedence, followed by the explicitly named file,
/// then a single `glass-lint.toml` or `glass-lint.json` in the current
/// directory. A discovered configuration is validated against the selected
/// provider catalog before it is returned.
pub fn load(args: &Args) -> Result<Config> {
    let (text, format) = if let Some(json) = &args.config_json {
        (json.clone(), "json")
    } else if let Some(path) = &args.config {
        let format = config_format(path)?;
        (
            fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?,
            format,
        )
    } else {
        match discover_from_cwd()? {
            Some(config) => config,
            None => return Ok(Config::default()),
        }
    };

    let raw: RawConfig = if format.eq_ignore_ascii_case("json") {
        serde_json::from_str(&text).context("parse JSON config")?
    } else {
        toml::from_str(&text).context("parse TOML config")?
    };
    let version = raw
        .version
        .ok_or_else(|| anyhow::anyhow!("config version is required"))?;
    if version != 2 {
        bail!("unsupported config version {version}; expected 2")
    }

    Config {
        version,
        core: raw.core,
        cli: raw.cli,
    }
    .validate()
}

fn config_format(path: &Path) -> Result<&'static str> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("toml") => Ok("toml"),
        Some("json") => Ok("json"),
        _ => bail!("config path must use .toml or .json: {}", path.display()),
    }
}

fn discover_from_cwd() -> Result<Option<(String, &'static str)>> {
    let cwd = std::env::current_dir()?;
    let toml = cwd.join("glass-lint.toml");
    let json = cwd.join("glass-lint.json");
    match (toml.exists(), json.exists()) {
        (true, true) => bail!("both {} and {} exist", toml.display(), json.display()),
        (true, false) => Ok(Some((fs::read_to_string(toml)?, "toml"))),
        (false, true) => Ok(Some((fs::read_to_string(json)?, "json"))),
        (false, false) => Ok(None),
    }
}

impl Config {
    fn validate(self) -> Result<Self> {
        self.project_load_options()?;
        if self.cli.pretty_max_width < 20 {
            bail!("pretty_max_width must be at least 20")
        }
        let catalog = catalog(self.cli.provider, self.cli.profile);
        self.core
            .validate(&catalog)
            .map_err(|error| anyhow::anyhow!("rule/provider mismatch: {error}"))?;
        Ok(self)
    }
}

impl Config {
    /// Construct and validate the project-loading policy used by linting
    /// modes.
    pub fn project_load_options(&self) -> Result<ValidatedProjectLoadOptions> {
        ProjectLoadOptions::builder()
            .max_source_bytes(self.cli.project.max_source_bytes)
            .max_project_source_bytes(self.cli.project.max_project_source_bytes)
            .max_visited_entries(self.cli.project.max_visited_entries)
            .max_timeout_ms(self.cli.project.max_timeout_ms)
            .build()
            .map_err(|error| anyhow::anyhow!(error))
    }

    /// Apply the CLI completion, diagnostic, and finding policy to one report.
    pub fn report_fails(&self, report: &glass_lint_core::AnalysisReport) -> bool {
        report.completion == glass_lint_core::ReportCompletion::Partial
            || !report.diagnostics.is_empty()
            || report.files.iter().any(|file| {
                file.has_parse_diagnostics()
                    || file
                        .findings
                        .iter()
                        .any(|finding| self.cli.fail_on.fails(finding.severity))
            })
    }
}

/// Build the complete immutable rule catalog selected by the CLI settings.
pub fn catalog(provider: Provider, profile: RuleSelectionProfile) -> RuleCatalog {
    base_linter(provider, profile).catalog().clone()
}

/// Construct the baseline linter for a provider and confidence profile;
/// selection is applied by `selected_linter`.
///
/// The combined Obsidian catalog shares Obsidian's host environment with its
/// generic JavaScript rules; the standalone JavaScript catalog uses its own
/// provider defaults.
pub fn base_linter(provider: Provider, profile: RuleSelectionProfile) -> Linter {
    let baseline = match profile {
        RuleSelectionProfile::Recommended => {
            RuleBaseline::MinimumConfidence(glass_lint_core::rules::Confidence::High)
        }
        RuleSelectionProfile::Heuristic => RuleBaseline::All,
    };
    let config = match provider {
        Provider::Obsidian => glass_lint_obsidian::obsidian_config(),
        Provider::Js => glass_lint_js::js_config(),
        Provider::Node => glass_lint_js::node_config(),
        Provider::Electron => glass_lint_js::electron_config(),
    };
    Linter::new(config.with_rules(RuleSelection::new(baseline)))
        .expect("built-in provider configuration is valid")
}

/// Construct and validate the linter requested by a complete CLI config.
///
/// Validation happens after catalog construction so rule selections and core
/// limits are checked against the exact provider environment that will run.
pub fn selected_linter(config: &Config) -> Result<Linter> {
    let profile_baseline = match config.cli.profile {
        RuleSelectionProfile::Recommended => {
            RuleBaseline::MinimumConfidence(glass_lint_core::rules::Confidence::High)
        }
        RuleSelectionProfile::Heuristic => RuleBaseline::All,
    };
    let selection = config.core.selection.overrides().iter().cloned().fold(
        RuleSelection::new(profile_baseline),
        RuleSelection::with_override,
    );
    let linter = base_linter(config.cli.provider, config.cli.profile);
    tracing::debug!(
        target: "glass_lint::cli",
        rules = linter.catalog().rule_ids().len(),
        "linter built"
    );
    Linter::new(
        glass_lint_core::LinterConfig::new(
            vec![linter.catalog().clone()],
            linter.analysis_environment().clone(),
        )
        .with_rules(selection)
        .with_limits(config.core.limits.clone()),
    )
    .map_err(|error| anyhow::anyhow!(error))
}

impl FailOn {
    /// Return whether a finding at `severity` should determine the process
    /// exit.
    pub fn fails(self, severity: Severity) -> bool {
        match self {
            Self::Info => true,
            Self::Warning => severity >= Severity::Warning,
            Self::Error => severity >= Severity::Error,
            Self::Never => false,
        }
    }
}

impl Verbosity {
    /// Map the CLI level to the core telemetry level without exposing core
    /// telemetry types in the serialized configuration schema.
    pub fn telemetry(self) -> glass_lint_core::telemetry::TelemetryLevel {
        match self {
            Self::Quiet => glass_lint_core::telemetry::TelemetryLevel::Quiet,
            Self::Normal => glass_lint_core::telemetry::TelemetryLevel::Normal,
            Self::Verbose => glass_lint_core::telemetry::TelemetryLevel::Verbose,
            Self::Trace => glass_lint_core::telemetry::TelemetryLevel::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsidian_profile_combines_generic_and_provider_rules() {
        let linter = base_linter(Provider::Obsidian, RuleSelectionProfile::Heuristic);
        let ids = linter.catalog().rule_ids();

        assert!(ids.iter().any(|id| id.as_str() == "js:dynamic-code.eval"));
        assert!(
            ids.iter()
                .any(|id| id.as_str() == "obsidian:markdown.code-block-processor")
        );
    }

    #[test]
    fn combined_obsidian_profile_uses_the_obsidian_host_environment() {
        let report = base_linter(Provider::Obsidian, RuleSelectionProfile::Heuristic)
            .lint_snippet(
                include_str!("../../tests/e2e/render-executable-code-blocks.js"),
                "render-executable-code-blocks.js",
            )
            .unwrap();
        let evals = report
            .files
            .iter()
            .flat_map(|file| file.findings.iter())
            .filter(|finding| finding.rule_id.as_str() == "js:dynamic-code.eval")
            .count();
        let processors = report
            .files
            .iter()
            .flat_map(|file| file.findings.iter())
            .filter(|finding| finding.rule_id.as_str() == "obsidian:markdown.code-block-processor")
            .count();

        assert_eq!(evals, 2);
        assert_eq!(processors, 2);
    }

    #[test]
    fn selected_linter_keeps_profile_baseline_before_core_overrides() {
        let mut recommended = Config::default();
        recommended.cli.provider = Provider::Js;
        recommended.cli.profile = RuleSelectionProfile::Recommended;
        let recommended = selected_linter(&recommended).unwrap();
        assert!(
            !recommended
                .enabled_rule_ids()
                .iter()
                .any(|id| id.as_str() == "js:dynamic-code.eval")
        );

        let mut override_config = Config::default();
        override_config.cli.provider = Provider::Js;
        override_config.cli.profile = RuleSelectionProfile::Recommended;
        override_config.core.selection = RuleSelection::new(RuleBaseline::All).with_override(
            glass_lint_core::RuleOverride::new(
                "js:dynamic-code.eval",
                glass_lint_core::RuleState::Enabled,
            )
            .unwrap(),
        );
        let overridden = selected_linter(&override_config).unwrap();
        assert!(
            overridden
                .enabled_rule_ids()
                .iter()
                .any(|id| id.as_str() == "js:dynamic-code.eval")
        );
    }

    #[test]
    fn project_timeout_is_validated_at_the_cli_boundary() {
        let mut config = Config::default();
        config.cli.project.max_timeout_ms = 0;
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("max_timeout_ms"));
    }

    #[test]
    fn legacy_flat_project_limits_are_rejected() {
        let error = serde_json::from_str::<RawConfig>(r#"{"version":2,"cli":{"max_bytes":1024}}"#)
            .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }
}
