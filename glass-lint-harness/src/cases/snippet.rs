use std::path::Path;

use anyhow::{Context, Result, bail};
use glass_lint_core::Severity;

use super::{default_filename, language_for_path};
use crate::types::{Case, ExpectedCount, FindingExpectation, ToolExpectation};

pub(super) fn parse_case(root: &Path, path: &Path, source: String) -> Result<Case> {
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
            "config" => config = Some(value),
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
    let mut certainty = None;
    for (key, value) in parse_fields(fields)? {
        match key.as_str() {
            "rule" => rule_id = Some(value),
            "severity" => severity = Some(parse_severity(&value)?),
            "count" => count = parse_expected_count(&value)?,
            "line" => expected_line = parse_optional_u32(&value)?,
            "column" => column = parse_optional_u32(&value)?,
            "message" => message = Some(value),
            "certainty" => certainty = Some(parse_certainty(&value)?),
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
    diagnostic.certainty = certainty;
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

fn parse_certainty(value: &str) -> Result<glass_lint_core::MatchCertainty> {
    match value {
        "definite" => Ok(glass_lint_core::MatchCertainty::Definite),
        "possible" => Ok(glass_lint_core::MatchCertainty::Possible),
        _ => bail!("unknown certainty `{value}`; expected definite or possible"),
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
