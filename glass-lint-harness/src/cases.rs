use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use glass_lint_core::Severity;
use walkdir::WalkDir;

use crate::types::{Case, DiagnosticExpectation, ToolExpectation};

fn default_language() -> String {
    "javascript".into()
}

fn default_filename() -> String {
    "main.js".into()
}

pub fn load_cases(root: &Path) -> Result<Vec<Case>> {
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "js")
        })
        .map(|entry| entry.into_path())
        .collect();
    paths.sort();

    let mut ids = BTreeSet::new();
    paths
        .into_iter()
        .map(|path| {
            let source =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let case = parse_case(root, &path, source)
                .with_context(|| format!("parse {}", path.display()))?;
            if case.language != "javascript" {
                bail!(
                    "{}: unsupported language `{}`",
                    path.display(),
                    case.language
                );
            }
            if !ids.insert(case.id.clone()) {
                bail!("duplicate case id `{}`", case.id);
            }
            Ok(case)
        })
        .collect()
}

fn parse_case(root: &Path, path: &Path, source: String) -> Result<Case> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let id = relative
        .with_extension("")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(default_filename);
    let mut case = Case {
        id: id.clone(),
        description: id,
        tags: vec![],
        language: default_language(),
        filename,
        source,
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
            add_expectation(&mut case, rest, line_number)?;
        } else if let Some(rest) = directive.strip_prefix("@expect-error ") {
            let line_number = if line[..comment_start].trim().is_empty() {
                (index + 2) as u32
            } else {
                (index + 1) as u32
            };
            add_expectation(&mut case, rest, line_number)?;
        }
    }

    case.source = strip_harness_comments(&case.source);
    Ok(case)
}

fn strip_harness_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let Some(comment_start) = line.find("// @") else {
                return line.to_owned();
            };
            let directive = line[comment_start + 3..].trim();
            if directive.starts_with("@case ")
                || directive.starts_with("@tool ")
                || directive.starts_with("@expect-error ")
                || directive.starts_with("@expect-error-after ")
            {
                format!(
                    "{}{}",
                    &line[..comment_start],
                    " ".repeat(line.len() - comment_start)
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        rules: vec![],
        required: vec![],
        forbidden: vec![],
    };
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
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
    if expectation.rules.is_empty() {
        bail!("@tool {name} must specify rules=");
    }
    case.tools.insert(name.into(), expectation);
    Ok(())
}

fn add_expectation(case: &mut Case, rest: &str, line: u32) -> Result<()> {
    let (tool, fields) = rest
        .split_once(' ')
        .with_context(|| format!("invalid @expect-error directive `{rest}`"))?;
    let expectation = case
        .tools
        .get_mut(tool)
        .with_context(|| format!("@expect-error references unconfigured tool `{tool}`"))?;
    let mut diagnostic = DiagnosticExpectation {
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
    expectation.required.push(diagnostic);
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
// @tool glass-lint rules=obsidian:dynamic_code
// @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
globalThis.setTimeout('run()', 10);
";
        let case = parse_case(
            Path::new("tests/cases"),
            Path::new("tests/cases/system/timer.js"),
            source.into(),
        )
        .unwrap();
        assert_eq!(case.id, "system/timer");
        assert_eq!(case.description, "Dynamic code");
        assert_eq!(case.tools["glass-lint"].required[0].line, Some(4));
    }
}
