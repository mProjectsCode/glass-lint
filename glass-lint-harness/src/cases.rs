#![allow(clippy::cast_possible_truncation)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{Severity, SourceLanguage};
use walkdir::WalkDir;

use crate::types::{
    Case, DiagnosticExpectation, ProjectCase, ProjectFile, ProjectResolution,
    ProjectResolutionResult, ToolExpectation,
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
    let relative = path.strip_prefix(root).unwrap_or(path);
    let id = relative
        .with_extension("")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let filename = path.file_name().map_or_else(
        || default_filename(path),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut case = Case {
        id: id.clone(),
        description: id,
        tags: vec![],
        language: language_for_path(path).into(),
        filename,
        source,
        project: None,
        tools: BTreeMap::new(),
    };

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
        if let Some(rest) = directive.strip_prefix("@expect-error-after ") {
            let line_number = previous_code_line(&lines, index)
                .with_context(|| format!("{}:{} has no previous code line", case.id, index + 1))?;
            add_expectation(&mut case, rest, line_number, true)?;
        } else if let Some(rest) = directive.strip_prefix("@expect-error ") {
            let line_number = if line[..comment_start].trim().is_empty() {
                (index + 2) as u32
            } else {
                (index + 1) as u32
            };
            add_expectation(&mut case, rest, line_number, true)?;
        } else if let Some(rest) = directive.strip_prefix("@expect-no-error-after ") {
            let line_number = previous_code_line(&lines, index)
                .with_context(|| format!("{}:{} has no previous code line", case.id, index + 1))?;
            add_expectation(&mut case, rest, line_number, false)?;
        } else if let Some(rest) = directive.strip_prefix("@expect-no-error ") {
            let line_number = if line[..comment_start].trim().is_empty() {
                (index + 2) as u32
            } else {
                (index + 1) as u32
            };
            add_expectation(&mut case, rest, line_number, false)?;
        }
    }

    Ok(case)
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

#[derive(Debug, Default, serde::Deserialize)]
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
    path: Option<String>,
    package: Option<String>,
    name: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    missing: bool,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ProjectToolManifest {
    config: Option<String>,
    #[serde(default)]
    rules: Vec<String>,
}

fn load_project_case(root: &Path, directory: &Path) -> Result<Case> {
    let manifest_path = directory.join("case.toml");
    let manifest: ProjectManifest = toml::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse {}", manifest_path.display()))?;
    let metadata = manifest.case.or(manifest.project).unwrap_or_default();
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

    let resolutions = manifest
        .resolution
        .into_iter()
        .map(|resolution| {
            let result = if resolution.missing {
                ProjectResolutionResult::Missing
            } else if let Some(path) = resolution.path {
                ProjectResolutionResult::Internal { path }
            } else if let Some(package) = resolution.package {
                ProjectResolutionResult::External { package }
            } else if let Some(name) = resolution.name {
                ProjectResolutionResult::Builtin { name }
            } else if let Some(reason) = resolution.reason {
                ProjectResolutionResult::Unsupported { reason }
            } else {
                bail!(
                    "resolution {} {} has no result",
                    resolution.importer,
                    resolution.request
                )
            };
            Ok(ProjectResolution {
                importer: resolution.importer,
                kind: resolution.kind,
                request: resolution.request,
                range: glass_lint_core::SourceRange {
                    start: glass_lint_core::Position {
                        line: resolution.line,
                        column: resolution.column,
                    },
                    end: glass_lint_core::Position {
                        line: resolution.end_line,
                        column: resolution.end_column,
                    },
                },
                result,
            })
        })
        .collect::<Result<Vec<_>>>()?;

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
            root: directory.to_owned(),
            entries,
            files,
            resolutions,
            filesystem: metadata.filesystem,
        }),
        tools,
    })
}

fn load_project_files(directory: &Path, paths: Vec<PathBuf>) -> Result<Vec<ProjectFile>> {
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(directory)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            Ok(ProjectFile {
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
    files: &[ProjectFile],
) -> Result<BTreeMap<String, ToolExpectation>> {
    let mut tools = BTreeMap::new();
    for (name, tool) in manifests {
        if tool.config.is_none() && tool.rules.is_empty() {
            bail!("project tool `{name}` must specify rules or config");
        }
        tools.insert(
            name.clone(),
            ToolExpectation {
                config: tool.config.clone(),
                rules: tool.rules.clone(),
                required: Vec::new(),
                forbidden: Vec::new(),
            },
        );
    }
    for file in files {
        let parsed = parse_case(directory, &directory.join(&file.path), file.source.clone())?;
        for (name, expectation) in parsed.tools {
            let entry = tools.entry(name).or_insert_with(|| ToolExpectation {
                config: None,
                rules: Vec::new(),
                required: Vec::new(),
                forbidden: Vec::new(),
            });
            if entry.config.is_none() {
                entry.config = expectation.config;
            }
            if entry.rules.is_empty() {
                entry.rules = expectation.rules;
            }
            entry
                .required
                .extend(expectation.required.into_iter().map(|mut expected| {
                    if expected.path.is_none() {
                        expected.path = Some(file.path.clone());
                    }
                    expected
                }));
            entry
                .forbidden
                .extend(expectation.forbidden.into_iter().map(|mut expected| {
                    if expected.path.is_none() {
                        expected.path = Some(file.path.clone());
                    }
                    expected
                }));
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
        .map(|(index, _)| (index + 1) as u32)
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
    let mut expectation = ToolExpectation {
        config: None,
        rules: vec![],
        required: vec![],
        forbidden: vec![],
    };
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "config" => {
                expectation.config = Some(value);
            }
            "rules" => {
                expectation.rules = value
                    .split(',')
                    .map(str::trim)
                    .filter(|rule| !rule.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => bail!("unknown @tool field `{key}`"),
        }
    }
    if expectation.config.is_none() && expectation.rules.is_empty() {
        bail!("@tool {name} must specify rules= or config=");
    }
    case.tools.insert(name.into(), expectation);
    Ok(())
}

fn add_expectation(case: &mut Case, rest: &str, line: u32, required: bool) -> Result<()> {
    let (tool, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @expect-error directive `{rest}`"))?;
    let expectation = case
        .tools
        .get_mut(tool)
        .with_context(|| format!("@expect-error references unconfigured tool `{tool}`"))?;
    let mut diagnostic = DiagnosticExpectation {
        path: None,
        rule_id: String::new(),
        message_id: None,
        severity: None,
        count: Some(1),
        line: Some(line),
        column: None,
        message: None,
    };
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "rule" => diagnostic.rule_id = value,
            "message_id" => diagnostic.message_id = Some(value),
            "severity" => diagnostic.severity = Some(parse_severity(&value)?),
            "count" => diagnostic.count = parse_optional_usize(&value)?,
            "line" => diagnostic.line = parse_optional_u32(&value)?,
            "column" => diagnostic.column = parse_optional_u32(&value)?,
            "message" => diagnostic.message = Some(value),
            _ => bail!("unknown @expect-error field `{key}`"),
        }
    }
    if diagnostic.rule_id.is_empty() {
        bail!("@expect-error for {tool} must specify rule=");
    }
    if required {
        expectation.required.push(diagnostic);
    } else {
        expectation.forbidden.push(diagnostic);
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

fn parse_optional_usize(value: &str) -> Result<Option<usize>> {
    if value == "any" {
        Ok(None)
    } else {
        Ok(Some(value.parse()?))
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
// @expect-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
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
        assert_eq!(case.tools["glass-lint"].required[0].line, Some(4));
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

        assert_eq!(case.tools["glass-lint"].forbidden.len(), 1);
        assert_eq!(case.tools["glass-lint"].forbidden[0].line, Some(3));
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
        let root = std::env::temp_dir().join(format!(
            "glass-lint-harness-language-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("conflict.ts"),
            "// @case language javascript\n// @tool glass-lint rules=js:network.request\nfetch('/remote');\n",
        )
        .unwrap();

        let error = load_cases(&root).unwrap_err().to_string();
        assert!(error.contains("conflicts with its fixture extension"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
