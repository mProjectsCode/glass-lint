use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use glass_lint_datastructures::{Position, SourceRange};

use super::{language_for_path, snippet::parse_case};
use crate::types::{
    AdapterFile, AdapterResolution, AdapterResolutionKind, AdapterResolutionResult, Case,
    ProjectCase, ToolExpectation,
};

#[derive(Debug, serde::Deserialize)]
struct ProjectManifest {
    case: Option<ProjectMetadata>,
    project: Option<ProjectMetadata>,
    #[serde(default)]
    resolution: Vec<ProjectResolutionManifest>,
    #[serde(default)]
    tool: BTreeMap<String, ProjectToolManifest>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct ProjectMetadata {
    id: Option<String>,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    entries: Vec<String>,
    #[serde(default)]
    filesystem: bool,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ProjectResolutionManifest {
    importer: String,
    kind: String,
    request: String,
    line: u32,
    column: u32,
    end_line: u32,
    end_column: u32,
    #[serde(flatten)]
    outcome: ManifestResolutionOutcome,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
enum ManifestResolutionOutcome {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(Debug, Default, serde::Deserialize)]
struct ProjectToolManifest {
    config: Option<String>,
    #[serde(default)]
    rules: Vec<String>,
}

fn parse_project_manifest(directory: &Path) -> Result<(ProjectManifest, ProjectMetadata)> {
    let manifest_path = directory.join("case.toml");
    let manifest: ProjectManifest = toml::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse {}", manifest_path.display()))?;
    let metadata = manifest
        .case
        .as_ref()
        .or(manifest.project.as_ref())
        .cloned()
        .unwrap_or_default();
    Ok((manifest, metadata))
}

fn build_resolutions(
    resolutions: Vec<ProjectResolutionManifest>,
) -> Result<Vec<AdapterResolution>> {
    resolutions
        .into_iter()
        .map(|resolution| {
            let result = match resolution.outcome {
                ManifestResolutionOutcome::Missing => AdapterResolutionResult::Missing,
                ManifestResolutionOutcome::Internal { path } => {
                    AdapterResolutionResult::Internal { path }
                }
                ManifestResolutionOutcome::External { package } => {
                    AdapterResolutionResult::External { package }
                }
                ManifestResolutionOutcome::Builtin { name } => {
                    AdapterResolutionResult::Builtin { name }
                }
                ManifestResolutionOutcome::OutsideProject { path } => {
                    AdapterResolutionResult::OutsideProject { path }
                }
                ManifestResolutionOutcome::Unsupported { reason } => {
                    AdapterResolutionResult::Unsupported { reason }
                }
            };
            Ok(AdapterResolution {
                importer: resolution.importer,
                kind: match resolution.kind.as_str() {
                    "import" => AdapterResolutionKind::Import,
                    "dynamic_import" | "dynamic-import" => AdapterResolutionKind::DynamicImport,
                    "require" => AdapterResolutionKind::Require,
                    other => bail!("unknown project request kind `{other}`"),
                },
                request: resolution.request,
                range: SourceRange::new(
                    Position::new(resolution.line, resolution.column)?,
                    Position::new(resolution.end_line, resolution.end_column)?,
                )?,
                result,
            })
        })
        .collect()
}

pub(super) fn load_project_case(root: &Path, directory: &Path) -> Result<Case> {
    let (manifest, metadata) = parse_project_manifest(directory)?;
    let relative_directory = directory.strip_prefix(root).unwrap_or(directory);
    let default_id = relative_directory.to_string_lossy().replace('\\', "/");

    let mut paths: Vec<_> = walkdir::WalkDir::new(directory)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name() != "case.toml"
                && super::is_supported_fixture_filename(entry.path())
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    paths.sort();
    let files = load_project_files(directory, paths)?;
    if files.is_empty() {
        bail!(
            "project case {} contains no runtime sources",
            directory.display()
        );
    }
    let entries = if metadata.entries.is_empty() {
        vec![files[0].path.clone()]
    } else {
        metadata.entries.clone()
    };

    let resolutions = build_resolutions(manifest.resolution)?;
    let tools = load_project_tools(directory, &manifest.tool, &files)?;
    let entry_source = entries
        .first()
        .and_then(|entry| files.iter().find(|file| &file.path == entry))
        .unwrap_or(&files[0]);
    Ok(Case {
        id: metadata.id.unwrap_or(default_id),
        description: metadata
            .description
            .unwrap_or_else(|| "multi-file project".into()),
        tags: metadata.tags,
        language: "project".into(),
        filename: entry_source.path.clone(),
        source: entry_source.source.clone(),
        project: Some(ProjectCase {
            protocol: crate::types::AdapterProject {
                root: directory.to_string_lossy().into_owned(),
                entries,
                files,
                resolutions,
            },
            filesystem: metadata.filesystem,
        }),
        adapters: tools,
    })
}

fn load_project_files(directory: &Path, paths: Vec<PathBuf>) -> Result<Vec<AdapterFile>> {
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(directory)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            Ok(AdapterFile {
                language: language_for_path(&path).into(),
                path: relative,
                source: fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?,
            })
        })
        .collect()
}

fn load_project_tools(
    directory: &Path,
    manifests: &BTreeMap<String, ProjectToolManifest>,
    files: &[AdapterFile],
) -> Result<BTreeMap<String, ToolExpectation>> {
    let mut tools = BTreeMap::new();
    for (name, tool) in manifests {
        if tool.config.is_none() && tool.rules.is_empty() {
            bail!("project tool `{name}` must specify rules or config");
        }
        tools.insert(
            name.clone(),
            ToolExpectation::new(tool.config.clone(), tool.rules.clone())
                .map_err(|error| anyhow::anyhow!("project tool `{name}`: {error}"))?,
        );
    }
    for file in files {
        let parsed = parse_case(directory, &directory.join(&file.path), file.source.clone())?;
        for (name, expectation) in parsed.adapters {
            let expectation = expectation.qualify_for_file(&file.path)?;
            if let Some(entry) = tools.get_mut(&name) {
                entry
                    .merge_from(expectation)
                    .map_err(|error| anyhow::anyhow!("project tool `{name}`: {error}"))?;
            } else {
                tools.insert(name, expectation);
            }
        }
    }
    Ok(tools)
}
