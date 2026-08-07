use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{Linter, RuleId};
use glass_lint_project::{ProjectLoadMetrics, ProjectLoadOutcome};

use crate::{
    builtins::{self, BuiltinProfile},
    profile::{
        config::{ProfileCatalogProvider, ProfileConfig, RuleSelectionProfile},
        corpus::{discover_profile_files, sample_paths},
        types::PreparedFile,
    },
    profile_manifest::verify_profile_manifest,
};

pub(super) fn selected_profile_paths(
    config: &ProfileConfig,
) -> Result<(Vec<PathBuf>, Option<String>, Option<u64>)> {
    if let Some(manifest) = &config.manifest {
        let root = config
            .paths
            .first()
            .context("manifest profiling requires one root")?;
        let verified = verify_profile_manifest(root, manifest)?;
        return Ok((
            verified.paths,
            Some(verified.digest),
            Some(verified.total_bytes),
        ));
    }
    let mut paths = discover_profile_files(&config.paths, &config.include, &config.exclude)?;
    if let Some(sample) = config.sample {
        sample_paths(&mut paths, sample, config.seed);
    }
    Ok((paths, None, None))
}

pub(super) fn prepare_file(path: &Path) -> Result<PreparedFile> {
    let metadata = fs::metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(PreparedFile {
        path: path.to_owned(),
        bytes: metadata.len(),
        source,
    })
}

pub(super) fn build_linters(
    provider: ProfileCatalogProvider,
    mode: RuleSelectionProfile,
    rules: &[String],
) -> Result<Vec<Arc<Linter>>> {
    let parsed = rules
        .iter()
        .map(|rule| RuleId::parse(rule.clone()).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    let providers = match provider {
        ProfileCatalogProvider::Js => vec![builtins::BuiltinProvider::Js],
        ProfileCatalogProvider::Obsidian => vec![builtins::BuiltinProvider::Obsidian],
        ProfileCatalogProvider::Both => vec![
            builtins::BuiltinProvider::Js,
            builtins::BuiltinProvider::Obsidian,
        ],
    };
    let mut linters = Vec::new();
    for provider in providers {
        let selected: Vec<_> = parsed
            .iter()
            .filter(|rule| provider.accepts_profile_rule(rule))
            .cloned()
            .collect();
        if !rules.is_empty() && selected.is_empty() {
            continue;
        }
        let profile = match mode {
            RuleSelectionProfile::Recommended => BuiltinProfile::Recommended,
            RuleSelectionProfile::Heuristic => BuiltinProfile::Heuristic,
        };
        let linter = if rules.is_empty() {
            builtins::linter(provider, profile)
        } else {
            builtins::linter_for_rules(provider, selected)?
        };
        linters.push(Arc::new(linter));
    }
    if linters.is_empty() {
        bail!("no selected rules belong to the chosen provider");
    }
    if parsed.iter().any(|rule| {
        ![
            builtins::BuiltinProvider::Js,
            builtins::BuiltinProvider::Obsidian,
        ]
        .iter()
        .any(|provider| provider.accepts_profile_rule(rule))
    }) {
        bail!("rule does not belong to a supported profiling provider");
    }
    Ok(linters)
}

pub(super) fn profile_project_parts(
    outcome: ProjectLoadOutcome,
) -> (
    glass_lint_core::project::AnalysisReport,
    ProjectLoadMetrics,
    Option<String>,
) {
    let partial_reason = outcome.status().reason().map(|error| format!("{error:#}"));
    (outcome.report, outcome.metrics, partial_reason)
}
