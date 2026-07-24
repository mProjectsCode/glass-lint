//! Typed tsconfig parsing, inheritance, reference traversal, and source
//! selection. Each phase consumes a typed predecessor; no phase uses a mutable
//! `serde_json::Value` as its semantic model.
//!
//! Phases:
//!   1. [`ParsedTsconfig`] — parsed fields from one file (no inheritance).
//!   2. [`MergedSelection`] — effective `files`/`include`/`exclude` after
//!      inheriting from a parent (consuming the parent by value).
//!   3. [`CompiledTsconfigSelection`] — [`MergedSelection`] compiled into a
//!      [`TsconfigPatternSet`], discarding raw selection strings. This is the
//!      production type used for source membership.

mod selection;

use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};

use serde_json::Value;

use crate::{admission::realpath, budget::ProjectResourceBudget, error::ProjectLoadError};

/// Budget for tsconfig traversal (extends and project references).
#[derive(Clone, Copy, Debug)]
pub struct ConfigTraversalBudget {
    /// Maximum number of config files to process across the whole traversal.
    pub max_config_count: usize,
    /// Maximum inheritance (extends or reference) chain depth.
    pub max_depth: usize,
}

impl ConfigTraversalBudget {
    pub const fn new(max_config_count: usize, max_depth: usize) -> Self {
        Self {
            max_config_count,
            max_depth,
        }
    }
}

impl Default for ConfigTraversalBudget {
    fn default() -> Self {
        Self {
            max_config_count: 100,
            max_depth: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Field-level representation for the parsed DTO
// ---------------------------------------------------------------------------

/// Generic parsed field representation distinguishing absent, null,
/// wrong-type, and present states.
#[derive(Clone, Debug)]
pub enum ParsedField<T> {
    Absent,
    Null,
    WrongType(String),
    Present(T),
}

pub type StringField = ParsedField<String>;
pub type StringArrayField = ParsedField<Vec<String>>;

impl<T> ParsedField<T> {
    fn ok(self) -> Option<T> {
        match self {
            Self::Present(v) => Some(v),
            _ => None,
        }
    }

    fn from_value_opt(value: Option<&Value>) -> Self
    where
        Self: FromValue,
    {
        value.map_or(Self::Absent, Self::from_value)
    }
}

trait FromValue: Sized {
    fn from_value(value: &Value) -> Self;
}

impl FromValue for ParsedField<String> {
    fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::String(s) => Self::Present(s.clone()),
            other => Self::WrongType(format!("expected string, got {}", type_name(other))),
        }
    }
}

impl FromValue for ParsedField<Vec<String>> {
    fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Array(arr) => {
                let mut items = Vec::with_capacity(arr.len());
                for v in arr {
                    match v.as_str() {
                        Some(s) => items.push(s.to_owned()),
                        None => {
                            return Self::WrongType(format!(
                                "expected string element in array, got {}",
                                type_name(v)
                            ));
                        }
                    }
                }
                Self::Present(items)
            }
            other => Self::WrongType(format!("expected array, got {}", type_name(other))),
        }
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Phase 1 — Parsed config fields
// ---------------------------------------------------------------------------

/// Typed representation of one parsed tsconfig file. Every supported field is
/// explicitly represented; unsupported fields are ignored.
///
/// This is the output of phase 1 (parsing) and the input to phase 2
/// (consuming inheritance).
#[derive(Clone, Debug)]
pub struct ParsedTsconfig {
    pub extends: StringField,
    pub files: StringArrayField,
    pub include: StringArrayField,
    pub exclude: StringArrayField,
    pub compiler_options_out_dir: StringField,
    pub compiler_options_declaration_dir: StringField,
    /// Typed project-reference entries; malformed entries are retained as
    /// diagnostics instead of disappearing during parsing.
    pub references: Vec<ReferenceEntry>,
    /// Field-level configuration diagnostics collected while parsing.
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceEntry {
    pub path: String,
}

impl ParsedTsconfig {
    /// Parse one config file's text (must already have JSONC comments
    /// stripped).
    pub fn parse(text: &str) -> Result<Self, String> {
        let parsed: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self::from_value(&parsed))
    }

    fn from_value(value: &Value) -> Self {
        let extends = StringField::from_value_opt(value.get("extends"));
        let files = StringArrayField::from_value_opt(value.get("files"));
        let include = StringArrayField::from_value_opt(value.get("include"));
        let exclude = StringArrayField::from_value_opt(value.get("exclude"));

        let compiler_options = value.get("compilerOptions");
        let compiler_options_out_dir = match compiler_options {
            Some(Value::Object(obj)) => StringField::from_value_opt(obj.get("outDir")),
            _ => StringField::Absent,
        };
        let compiler_options_declaration_dir = match compiler_options {
            Some(Value::Object(obj)) => StringField::from_value_opt(obj.get("declarationDir")),
            _ => StringField::Absent,
        };

        let mut diagnostics = Vec::new();
        for (name, message) in [
            ("extends", extends.error()),
            ("files", files.error()),
            ("include", include.error()),
            ("exclude", exclude.error()),
        ] {
            if let Some(message) = message {
                diagnostics.push(format!("{name}: {message}"));
            }
        }

        let references = match value.get("references") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|reference| match reference {
                    Value::Object(object) => match object.get("path") {
                        Some(Value::String(path)) => Some(ReferenceEntry { path: path.clone() }),
                        Some(other) => {
                            diagnostics.push(format!(
                                "references.path: expected string, got {}",
                                type_name(other)
                            ));
                            None
                        }
                        None => {
                            diagnostics.push("references: entry is missing path".into());
                            None
                        }
                    },
                    other => {
                        diagnostics.push(format!(
                            "references: expected object entry, got {}",
                            type_name(other)
                        ));
                        None
                    }
                })
                .collect(),
            Some(other) => {
                diagnostics.push(format!(
                    "references: expected array, got {}",
                    type_name(other)
                ));
                Vec::new()
            }
        };

        Self {
            extends,
            files,
            include,
            exclude,
            compiler_options_out_dir,
            compiler_options_declaration_dir,
            references,
            diagnostics,
        }
    }
}

trait FieldState {
    fn error(&self) -> Option<String>;
}

impl<T> FieldState for ParsedField<T> {
    fn error(&self) -> Option<String> {
        match self {
            Self::WrongType(message) => Some(message.clone()),
            Self::Null => Some("value is null".into()),
            Self::Absent | Self::Present(_) => None,
        }
    }
}

// re-export selection types
#[allow(unused_imports)]
pub use selection::{
    CompiledTsconfigSelection, MergedSelection, TsconfigPatternSet, merge_selection,
};

// ---------------------------------------------------------------------------
// Cycle detection diagnostics
// ---------------------------------------------------------------------------

/// Structured diagnostic for a detected cycle or malformed configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TsconfigDiagnostic {
    /// The canonical config path where the cycle was detected.
    pub config_path: PathBuf,
    /// The canonical path of the parent/reference that created the cycle.
    /// `None` when the diagnostic is not about a cycle (e.g. parse errors or
    /// pattern issues).
    pub cycle_target: Option<PathBuf>,
    /// Human-readable description.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Phase functions
// ---------------------------------------------------------------------------

/// Phase 1: Read and parse one tsconfig file (JSONC-aware), bounded by the
/// project resource budget's config byte limit.
pub fn read_and_parse(
    config_path: &Path,
    budget: &mut ProjectResourceBudget,
) -> Result<ParsedTsconfig, ProjectLoadError> {
    let file = std::fs::File::open(config_path).map_err(|source| ProjectLoadError::Io {
        path: config_path.to_path_buf(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| ProjectLoadError::Io {
        path: config_path.to_path_buf(),
        source,
    })?;
    budget.record_config_bytes(metadata.len())?;
    let mut text = String::new();
    file.take(budget.max_config_bytes().saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|source| ProjectLoadError::Io {
            path: config_path.to_path_buf(),
            source,
        })?;
    if text.len() as u64 > budget.max_config_bytes() {
        return Err(ProjectLoadError::ProjectSourceTooLarge {
            bytes: text.len() as u64,
            limit: budget.max_config_bytes(),
        });
    }
    json_strip_comments::strip(&mut text).map_err(|error| parse_error(config_path, error))?;
    ParsedTsconfig::parse(&text).map_err(|error| parse_error(config_path, error))
}

fn parse_error(config: &Path, error: impl fmt::Display) -> ProjectLoadError {
    ProjectLoadError::ConfigParseError {
        path: config.to_path_buf(),
        source: error.to_string(),
    }
}

/// Resolve an `extends` string relative to the config's directory.
/// Returns None for package-based extends that should be ignored.
fn resolve_extends(config_path: &Path, extends: &str) -> Option<PathBuf> {
    if !extends.starts_with('.') && !Path::new(extends).is_absolute() {
        return None;
    }
    let base = config_path.parent()?;
    let mut path = if Path::new(extends).is_absolute() {
        PathBuf::from(extends)
    } else {
        base.join(extends)
    };
    if path.extension().is_none() {
        path.set_extension("json");
    }
    Some(path)
}

/// Phase 2-3: Build the effective selection for a given config path, resolving
/// `extends` inheritance recursively. Returns diagnostics for cycles.
///
/// Maintains an internal extends-chain to detect cycles without affecting the
/// caller's visited set. When a cycle is found, a diagnostic is emitted, the
/// offending `extends` edge is discarded, and the current file's local settings
/// plus any already-resolved acyclic ancestors are retained. The child config
/// continues building normally without the cyclic parent. Independent
/// non-cyclic branches continue unaffected.
#[allow(clippy::too_many_arguments)]
pub fn build_effective_config(
    config_path: &Path,
    fallback_base: &Path,
    deadline: Option<Instant>,
    diagnostics: &mut Vec<TsconfigDiagnostic>,
    budget: ConfigTraversalBudget,
    config_count: &mut usize,
    resource_budget: &mut ProjectResourceBudget,
) -> Result<(selection::CompiledTsconfigSelection, Vec<ReferenceEntry>), ProjectLoadError> {
    let mut extends_chain: Vec<PathBuf> = Vec::new();
    let (merged, references) = build_effective_config_inner(
        config_path,
        fallback_base,
        &mut extends_chain,
        deadline,
        diagnostics,
        budget,
        config_count,
        resource_budget,
    )?;
    let canonical = realpath(config_path)?;
    let compiled = selection::CompiledTsconfigSelection::compile(canonical, merged);
    diagnostics.extend(
        compiled
            .pattern_diagnostics
            .iter()
            .map(|message| TsconfigDiagnostic {
                config_path: compiled.config_path.clone(),
                cycle_target: None,
                message: message.clone(),
            }),
    );
    Ok((compiled, references))
}

#[allow(clippy::too_many_arguments)]
fn build_effective_config_inner(
    config_path: &Path,
    fallback_base: &Path,
    extends_chain: &mut Vec<PathBuf>,
    deadline: Option<Instant>,
    diagnostics: &mut Vec<TsconfigDiagnostic>,
    budget: ConfigTraversalBudget,
    config_count: &mut usize,
    resource_budget: &mut ProjectResourceBudget,
) -> Result<(selection::MergedSelection, Vec<ReferenceEntry>), ProjectLoadError> {
    if let Some(deadline) = deadline
        && Instant::now() >= deadline
    {
        return Err(ProjectLoadError::Timeout);
    }
    let canonical = realpath(config_path)?;

    // Depth check (extends chain)
    if extends_chain.len() >= budget.max_depth {
        return Err(ProjectLoadError::ConfigBudgetExhausted {
            kind: "extends depth",
            limit: budget.max_depth,
        });
    }
    extends_chain.push(canonical.clone());

    // Config count check
    *config_count += 1;
    if *config_count > budget.max_config_count {
        return Err(ProjectLoadError::ConfigBudgetExhausted {
            kind: "config count",
            limit: budget.max_config_count,
        });
    }

    let dto = read_and_parse(config_path, resource_budget)?;
    for message in &dto.diagnostics {
        diagnostics.push(TsconfigDiagnostic {
            config_path: canonical.clone(),
            cycle_target: None,
            message: message.clone(),
        });
    }
    let base = config_path.parent().unwrap_or(fallback_base).to_path_buf();

    // Resolve extends — detect cycles at the extends-resolution site rather
    // than returning a sentinel config that callers must recognise.
    let references = dto.references.clone();
    let parent_merged = dto
        .extends
        .clone()
        .ok()
        .and_then(|extends_str| {
            let parent_path = resolve_extends(config_path, &extends_str)
                .filter(|parent_path| parent_path.exists());
            parent_path.and_then(|parent_path| {
                // Canonicalize before cycle comparison so equivalent
                // paths containing .. or symlink aliases are caught.
                match realpath(&parent_path) {
                    Ok(parent_canonical) => {
                        if extends_chain.contains(&parent_canonical) {
                            diagnostics.push(TsconfigDiagnostic {
                                config_path: canonical.clone(),
                                cycle_target: Some(parent_canonical),
                                message: format!(
                                    "cycle detected: {} is already in the inheritance chain",
                                    canonical.display()
                                ),
                            });
                            None
                        } else {
                            let result = build_effective_config_inner(
                                &parent_canonical,
                                &base,
                                extends_chain,
                                deadline,
                                diagnostics,
                                budget,
                                config_count,
                                resource_budget,
                            );
                            Some(result.map(|(merged, _)| merged))
                        }
                    }
                    Err(e) => Some(Err(e)),
                }
            })
        })
        .transpose()?;

    extends_chain.pop();

    // Merge: consume child dto and optional parent MergedSelection.
    // No cloning of selection data occurs — owned fields are moved.
    let effective = selection::merge_selection(dto, parent_merged);
    Ok((effective, references))
}

#[cfg(test)]
mod tests;
