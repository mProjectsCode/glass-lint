//! Fixture discovery and directive/manifest parsing.
//!
//! Case IDs and file order are normalized before execution so reports and
//! adapter requests remain stable across filesystem traversal implementations.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{Severity, SourceLanguage};
use walkdir::WalkDir;

use crate::types::{
    AdapterFile, AdapterResolution, AdapterResolutionKind, AdapterResolutionResult, Case,
    ExpectedCount, FindingExpectation, ProjectCase, ToolExpectation,
};

fn language_for_path(path: &Path) -> &'static str {
    match SourceLanguage::from_filename(&path.to_string_lossy()) {
        SourceLanguage::TypeScript => "typescript",
        SourceLanguage::JavaScript => "javascript",
    }
}

fn default_filename(path: &Path) -> String {
    path.file_name().map_or_else(
        || "main.js".into(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub fn load_cases(root: &Path) -> Result<Vec<Case>> {
    // Project manifests claim their whole directory; ordinary source files
    // beneath those directories must not be loaded as duplicate cases.
    let mut project_directories = BTreeSet::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.file_name() == "case.toml" {
            project_directories.insert(entry.path().parent().unwrap_or(root).to_owned());
        }
    }
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && SourceLanguage::is_supported_filename(&entry.path().to_string_lossy())
                && !project_directories
                    .iter()
                    .any(|directory| entry.path().starts_with(directory))
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    paths.sort();

    let mut cases = paths
        .into_iter()
        .map(|path| {
            let source =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let case = parse_case(root, &path, source)
                .with_context(|| format!("parse {}", path.display()))?;
            let expected_language = language_for_path(&path);
            if case.language != expected_language {
                bail!(
                    "{}: language `{}` conflicts with its fixture extension (expected `{}`)",
                    path.display(),
                    case.language,
                    expected_language
                );
            }
            Ok(case)
        })
        .collect::<Result<Vec<_>>>()?;
    for directory in project_directories {
        cases.push(load_project_case(root, &directory)?);
    }
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    let mut ids = BTreeSet::new();
    for case in &cases {
        if !ids.insert(case.id.clone()) {
            bail!("duplicate case id `{}`", case.id);
        }
    }
    Ok(cases)
}

fn parse_case(root: &Path, path: &Path, source: String) -> Result<Case> {
    // Directives are read only from leading comments, while expectation lines
    // may be attached to code and therefore use their preceding line rules.
    let relative = path.strip_prefix(root).unwrap_or(path);
    let id = relative
        .with_extension("")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let filename = path.file_name().map_or_else(
        || default_filename(path),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut case = Case::new(id.clone(), id, language_for_path(path), filename, source)
        .map_err(|error| anyhow::anyhow!(error))?;

    let lines: Vec<_> = case.source.lines().map(str::to_owned).collect();
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let Some(comment) = trimmed.strip_prefix("//") else {
            break;
        };
        let directive = comment.trim();
        if let Some(rest) = directive.strip_prefix("@case ") {
            parse_case_directive(&mut case, rest)?;
        } else if let Some(rest) = directive.strip_prefix("@tool ") {
            parse_tool_directive(&mut case, rest)?;
        }
    }

    for (index, line) in lines.iter().enumerate() {
        let Some(comment_start) = line.find("// @") else {
            continue;
        };
        let directive = line[comment_start + 3..].trim();
        if let Some((rest, after, required)) = expectation_directive(directive) {
            let line_number = if after {
                previous_code_line(&lines, index).with_context(|| {
                    format!("{}:{} has no previous code line", case.id, index + 1)
                })?
            } else if line[..comment_start].trim().is_empty() {
                u32::try_from(index + 2).context("fixture line number exceeds u32")?
            } else {
                u32::try_from(index + 1).context("fixture line number exceeds u32")?
            };
            add_expectation(&mut case, rest, line_number, required)?;
        }
    }

    Ok(case)
}

fn expectation_directive(directive: &str) -> Option<(&str, bool, bool)> {
    [
        ("@expect-error-after ", true, true),
        ("@expect-error ", false, true),
        ("@expect-no-error-after ", true, false),
        ("@expect-no-error ", false, false),
    ]
    .into_iter()
    .find_map(|(prefix, after, required)| {
        directive
            .strip_prefix(prefix)
            .map(|rest| (rest, after, required))
    })
}

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
struct ProjectResolutionManifest {
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
                range: glass_lint_datastructures::SourceRange::new(
                    glass_lint_datastructures::Position::new(resolution.line, resolution.column)?,
                    glass_lint_datastructures::Position::new(
                        resolution.end_line,
                        resolution.end_column,
                    )?,
                )?,
                result,
            })
        })
        .collect()
}

fn load_project_case(root: &Path, directory: &Path) -> Result<Case> {
    let (manifest, metadata) = parse_project_manifest(directory)?;
    let relative_directory = directory.strip_prefix(root).unwrap_or(directory);
    let default_id = relative_directory.to_string_lossy().replace('\\', "/");

    let mut paths: Vec<_> = WalkDir::new(directory)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name() != "case.toml"
                && SourceLanguage::is_supported_filename(&entry.path().to_string_lossy())
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
            let (selector, required, forbidden) = expectation.into_parts();
            let requirements = required
                .into_iter()
                .map(|mut expected| {
                    if expected.path.is_none() {
                        expected.path = Some(
                            glass_lint_core::project::ProjectRelativePath::new(file.path.clone())
                                .map_err(|error| anyhow::anyhow!(error))?,
                        );
                    }
                    Ok::<_, anyhow::Error>(expected)
                })
                .collect::<Result<Vec<_>>>()?;
            let forbidden = forbidden
                .into_iter()
                .map(|mut expected| {
                    if expected.path.is_none() {
                        expected.path = Some(
                            glass_lint_core::project::ProjectRelativePath::new(file.path.clone())
                                .map_err(|error| anyhow::anyhow!(error))?,
                        );
                    }
                    Ok::<_, anyhow::Error>(expected)
                })
                .collect::<Result<Vec<_>>>()?;
            let expectation = ToolExpectation::from_selector(selector, requirements, forbidden)
                .map_err(anyhow::Error::msg)?;
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

fn previous_code_line(lines: &[String], assertion_index: usize) -> Option<u32> {
    lines[..assertion_index]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("@expect-error")
                && !trimmed.starts_with("@expect-error-after")
        })
        .map(|(index, _)| u32::try_from(index + 1).unwrap_or(u32::MAX))
}

fn parse_case_directive(case: &mut Case, rest: &str) -> Result<()> {
    let (key, value) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @case directive `{rest}`"))?;
    match key {
        "id" => case.id = value.trim().into(),
        "description" => case.description = value.trim().into(),
        "tags" => {
            case.tags = value
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect();
        }
        "filename" => case.filename = value.trim().into(),
        "language" => case.language = value.trim().into(),
        _ => bail!("unknown @case key `{key}`"),
    }
    Ok(())
}

fn parse_tool_directive(case: &mut Case, rest: &str) -> Result<()> {
    let (name, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @tool directive `{rest}`"))?;
    let mut config = None;
    let mut rules = Vec::new();
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "config" => {
                config = Some(value);
            }
            "rules" => {
                rules = value
                    .split(',')
                    .map(str::trim)
                    .filter(|rule| !rule.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => bail!("unknown @tool field `{key}`"),
        }
    }
    let expectation = ToolExpectation::new(config, rules)
        .map_err(|error| anyhow::anyhow!("@tool {name}: {error}"))?;
    case.adapters.insert(name.into(), expectation);
    Ok(())
}

fn add_expectation(case: &mut Case, rest: &str, line: u32, required: bool) -> Result<()> {
    let (tool, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @expect-error directive `{rest}`"))?;
    let expectation = case
        .adapters
        .get_mut(tool)
        .with_context(|| format!("@expect-error references unconfigured tool `{tool}`"))?;
    let mut rule_id = None;
    let mut severity = None;
    let mut count = ExpectedCount::Exactly(1);
    let mut expected_line = Some(line);
    let mut column = None;
    let mut message = None;
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "rule" => rule_id = Some(value),
            "severity" => severity = Some(parse_severity(&value)?),
            "count" => count = parse_expected_count(&value)?,
            "line" => expected_line = parse_optional_u32(&value)?,
            "column" => column = parse_optional_u32(&value)?,
            "message" => message = Some(value),
            _ => bail!("unknown @expect-error field `{key}`"),
        }
    }
    let mut diagnostic = FindingExpectation::new(
        rule_id.with_context(|| format!("@expect-error for {tool} must specify rule="))?,
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    diagnostic.severity = severity;
    diagnostic.count = count;
    diagnostic.line = expected_line;
    diagnostic.column = column;
    diagnostic.message = message;
    if required {
        expectation.add_required(diagnostic);
    } else {
        expectation.add_forbidden(diagnostic);
    }
    Ok(())
}

fn parse_fields(fields: &str) -> Result<Vec<(String, String)>> {
    fields
        .split_whitespace()
        .map(|field| {
            let (key, value) = field
                .split_once('=')
                .with_context(|| format!("expected key=value, found `{field}`"))?;
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn parse_severity(value: &str) -> Result<Severity> {
    match value {
        "info" => Ok(Severity::Info),
        "warning" => Ok(Severity::Warning),
        "error" => Ok(Severity::Error),
        _ => bail!("unknown severity `{value}`"),
    }
}

fn parse_optional_u32(value: &str) -> Result<Option<u32>> {
    if value == "any" {
        Ok(None)
    } else {
        Ok(Some(value.parse()?))
    }
}

fn parse_expected_count(value: &str) -> Result<ExpectedCount> {
    if value == "any" {
        Ok(ExpectedCount::AtLeastOne)
    } else {
        Ok(ExpectedCount::Exactly(value.parse()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comment_case() {
        let source = "\
// @case description Dynamic code
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer
globalThis.setTimeout('run()', 10);
";
        let case = parse_case(
            Path::new("fixtures"),
            Path::new("fixtures/system/timer.js"),
            source.into(),
        )
        .unwrap();
        assert_eq!(case.id, "system/timer");
        assert_eq!(case.description, "Dynamic code");
        assert_eq!(case.adapters["glass-lint"].required()[0].line, Some(4));
    }

    #[test]
    fn parses_forbidden_diagnostic() {
        let source = "\
// @tool glass-lint rules=js:network.request
fetch('/remote'); // @expect-error glass-lint rule=js:network.request
function local(fetch) { fetch('/local'); } // @expect-no-error glass-lint rule=js:network.request
";
        let case = parse_case(
            Path::new("fixtures"),
            Path::new("fixtures/network/precision.js"),
            source.into(),
        )
        .unwrap();

        assert_eq!(case.adapters["glass-lint"].forbidden().len(), 1);
        assert_eq!(case.adapters["glass-lint"].forbidden()[0].line, Some(3));
    }

    #[test]
    fn defaults_typescript_cases_from_the_fixture_extension() {
        let case = parse_case(
            Path::new("fixtures"),
            Path::new("fixtures/network/runtime.mts"),
            "// @tool glass-lint rules=js:network.request\nfetch('/remote');\n".into(),
        )
        .unwrap();

        assert_eq!(case.language, "typescript");
        assert_eq!(case.filename, "runtime.mts");
    }

    #[test]
    fn rejects_a_language_that_conflicts_with_the_fixture_extension() {
        let root = crate::test_support::TempDir::new();
        std::fs::write(
            root.path().join("conflict.ts"),
            "// @case language javascript\n// @tool glass-lint rules=js:network.request\nfetch('/remote');\n",
        )
        .unwrap();

        let error = load_cases(root.path()).unwrap_err().to_string();
        assert!(error.contains("conflicts with its fixture extension"));
    }

    #[test]
    fn rejects_legacy_competing_resolution_fields() {
        let error = toml::from_str::<ProjectResolutionManifest>(
            "importer = 'main.js'\nkind = 'import'\nrequest = 'pkg'\nline = 1\ncolumn = 1\nend_line = 1\nend_column = 4\npath = 'src/pkg.js'\npackage = 'pkg'\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("outcome"));
    }
}
