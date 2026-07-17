//! Configuration schema, loading precedence, and provider/profile selection.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use glass_lint_core::{CoreConfig, Linter, MAX_SOURCE_BYTES, RuleCatalog, Severity};
use serde::{Deserialize, Serialize};

use crate::args::Args;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Obsidian rules together with the generic JavaScript catalog.
    #[default]
    Obsidian,
    Js,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
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
pub enum Output {
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
    pub profile: Profile,
    #[serde(default = "default_max_bytes")]
    /// Maximum source size accepted by project discovery, in bytes.
    pub max_bytes: u64,
    #[serde(default = "default_project_bytes")]
    pub max_project_bytes: u64,
    #[serde(default = "default_visited_entries")]
    pub max_visited_entries: usize,
    #[serde(default)]
    /// Minimum finding severity that makes the command fail.
    pub fail_on: FailOn,
    #[serde(default)]
    /// Serialization format for findings and summaries.
    pub output: Output,
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
            profile: Profile::default(),
            max_bytes: default_max_bytes(),
            max_project_bytes: default_project_bytes(),
            max_visited_entries: default_visited_entries(),
            fail_on: FailOn::default(),
            output: Output::default(),
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
            version: 1,
            core: CoreConfig::default(),
            cli: CliConfig::default(),
        }
    }
}

#[derive(Deserialize)]
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
    if version != 1 {
        bail!("unsupported config version {version}; expected 1")
    }

    validate(Config {
        version,
        core: raw.core,
        cli: raw.cli,
    })
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

fn validate(config: Config) -> Result<Config> {
    if config.cli.max_bytes == 0 || config.cli.max_bytes > MAX_SOURCE_BYTES as u64 {
        bail!("max_bytes must be between 1 and {MAX_SOURCE_BYTES}")
    }
    if config.cli.max_project_bytes < config.cli.max_bytes {
        bail!("max_project_bytes must be at least max_bytes")
    }
    if config.cli.max_visited_entries == 0 {
        bail!("visited limit must be positive")
    }
    if config.cli.pretty_max_width < 20 {
        bail!("pretty_max_width must be at least 20")
    }
    let catalog = catalog(config.cli.provider, config.cli.profile);
    config
        .core
        .validate(&catalog)
        .map_err(|error| anyhow::anyhow!("rule/provider mismatch: {error}"))?;
    Ok(config)
}

/// Build the immutable rule metadata selected by the CLI settings.
pub fn catalog(provider: Provider, profile: Profile) -> RuleCatalog {
    base_linter(provider, profile).catalog().clone()
}

/// Construct the unconfigured catalog for a provider and confidence profile.
///
/// The combined Obsidian catalog shares Obsidian's host environment with its
/// generic JavaScript rules; the standalone JavaScript catalog uses its own
/// provider defaults.
pub fn base_linter(provider: Provider, profile: Profile) -> Linter {
    match (provider, profile) {
        (Provider::Obsidian, profile) => {
            let environment = glass_lint_obsidian::default_environment();
            let js = match profile {
                Profile::Recommended => {
                    glass_lint_js::recommended_linter_with_environment(environment.clone())
                }
                Profile::Heuristic => {
                    glass_lint_js::heuristic_linter_with_environment(environment.clone())
                }
            };
            let obsidian = match profile {
                Profile::Recommended => {
                    glass_lint_obsidian::recommended_linter_with_environment(environment.clone())
                }
                Profile::Heuristic => {
                    glass_lint_obsidian::heuristic_linter_with_environment(environment.clone())
                }
            };
            Linter::combine_with_environment([js, obsidian], environment)
                .expect("built-in provider catalogs have unique namespaced rule IDs")
        }
        (Provider::Js, Profile::Recommended) => glass_lint_js::recommended_linter(),
        (Provider::Js, Profile::Heuristic) => glass_lint_js::heuristic_linter(),
    }
}

/// Construct and validate the linter requested by a complete CLI config.
///
/// Validation happens after catalog construction so rule selections and core
/// limits are checked against the exact provider environment that will run.
pub fn selected_linter(config: &Config) -> Result<Linter> {
    let linter = base_linter(config.cli.provider, config.cli.profile);
    tracing::debug!(
        target: "glass_lint::cli",
        rules = linter.catalog().rule_ids().len(),
        "linter built"
    );
    linter
        .configured(&config.core)
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
        let linter = base_linter(Provider::Obsidian, Profile::Heuristic);
        let ids = linter.catalog().rule_ids();

        assert!(ids.iter().any(|id| id.as_str() == "js:dynamic-code.eval"));
        assert!(
            ids.iter()
                .any(|id| id.as_str() == "obsidian:markdown.code-block-processor")
        );
    }

    #[test]
    fn combined_obsidian_profile_uses_the_obsidian_host_environment() {
        let report = base_linter(Provider::Obsidian, Profile::Heuristic)
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
}
