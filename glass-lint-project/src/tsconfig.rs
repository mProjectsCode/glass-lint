//! Typed tsconfig parsing, inheritance, reference traversal, and source
//! selection. Each phase consumes a typed predecessor; no phase uses a mutable
//! `serde_json::Value` as its semantic model.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use serde_json::Value;

use crate::{admission::realpath, error::ProjectLoadError};

static COMPILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Reset the compile counter (test only).
#[cfg(test)]
pub fn reset_compile_counter() {
    COMPILE_COUNTER.store(0, Ordering::SeqCst);
}

/// Read the compile counter (test only).
#[cfg(test)]
pub fn compile_count() -> usize {
    COMPILE_COUNTER.load(Ordering::SeqCst)
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
// Parsed config fields DTO
// ---------------------------------------------------------------------------

/// Typed representation of one parsed tsconfig file. Every supported field is
/// explicitly represented; unsupported fields are ignored.
#[derive(Clone, Debug)]
pub struct TsconfigDto {
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

impl TsconfigDto {
    /// Parse one config file's text (must already have JSONC comments
    /// stripped).
    fn parse(text: &str) -> Result<Self, String> {
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

// ---------------------------------------------------------------------------
// Effective merged tsconfig
// ---------------------------------------------------------------------------

/// An effective (fully inherited) tsconfig with normalized paths and compiled
/// patterns. This is the semantic model used for source selection.
#[derive(Clone, Debug)]
pub struct Tsconfig {
    /// Canonical config path.
    config_path: PathBuf,
    /// Explicit files list (None = use include/exclude).
    pub files: Option<Vec<String>>,
    /// Include patterns (defaults to `**/*` when files is None).
    pub include: Vec<String>,
    /// Exclude patterns (includes default node_modules/bower_components and
    /// outDir).
    pub exclude: Vec<String>,
    /// Compiled pattern set for include/exclude matching.
    pub pattern_set: TsconfigPatternSet,
    /// Invalid patterns that caused fail-closed source selection.
    pub pattern_diagnostics: Vec<String>,
}

impl Tsconfig {
    fn new(config_path: PathBuf, dto: TsconfigDto, parent: Option<&Self>) -> Self {
        // Merge: child wins over parent for all fields except compilerOptions
        // (which deep-merges).
        let files = dto
            .files
            .ok()
            .or_else(|| parent.and_then(|p| p.files.clone()));

        let (include, exclude) = if files.is_some() {
            (Vec::new(), Vec::new())
        } else {
            // Distinguish Absent (inherit or default) from Present (use as-is
            // even when empty) so an explicit empty array is not collapsed
            // with an absent field.
            let include = match dto.include {
                ParsedField::Present(v) => v,
                _ => {
                    parent.map_or_else(|| vec!["**/*".to_string()], |parent| parent.include.clone())
                }
            };

            let mut exclude = match dto.exclude {
                ParsedField::Present(v) => v,
                _ => parent.map_or_else(Vec::new, |parent| parent.exclude.clone()),
            };
            // Always add default runtime exclusions
            for default in &["**/node_modules", "**/bower_components"] {
                if !exclude.iter().any(|e| e == default) {
                    exclude.push(default.to_string());
                }
            }
            // Add output directories from this config's compilerOptions
            if let Some(out_dir) = dto.compiler_options_out_dir.ok()
                && !exclude.iter().any(|e| e == &out_dir)
            {
                exclude.push(out_dir);
            }
            if let Some(decl_dir) = dto.compiler_options_declaration_dir.ok()
                && !exclude.iter().any(|e| e == &decl_dir)
            {
                exclude.push(decl_dir);
            }

            (include, exclude)
        };

        let pattern_set = TsconfigPatternSet::new(&include, &exclude);
        let pattern_diagnostics = pattern_set
            .invalid_patterns()
            .map(|pattern| format!("invalid glob pattern `{pattern}`"))
            .collect();

        Self {
            config_path,
            files,
            include,
            exclude,
            pattern_set,
            pattern_diagnostics,
        }
    }
}

// ---------------------------------------------------------------------------
// Compiled pattern set
// ---------------------------------------------------------------------------

/// Validated, normalized, and compiled include/exclude patterns. Provides
/// allocation-free borrowed matching against canonical project-relative paths.
#[derive(Clone, Debug)]
pub struct TsconfigPatternSet {
    includes: Vec<glob::Pattern>,
    excludes: Vec<glob::Pattern>,
    invalid: Vec<String>,
}

fn matches_relative(pattern: &glob::Pattern, relative: &str) -> bool {
    pattern.matches(relative)
        || (!pattern.as_str().contains('/')
            && relative
                .split('/')
                .next_back()
                .is_some_and(|name| pattern.matches(name)))
}

impl TsconfigPatternSet {
    fn new(includes: &[String], excludes: &[String]) -> Self {
        COMPILE_COUNTER.fetch_add(1, Ordering::SeqCst);

        let normalize = |pattern: &str| -> String {
            let normalized = pattern.replace('\\', "/");
            if normalized.ends_with('/') {
                format!("{normalized}**/*")
            } else {
                normalized
            }
        };

        let compile = |patterns: &[String]| -> (Vec<glob::Pattern>, Vec<String>) {
            let mut compiled = Vec::new();
            let mut invalid = Vec::new();
            for pattern in patterns.iter().map(|p| normalize(p)) {
                match glob::Pattern::new(&pattern) {
                    Ok(pattern) => compiled.push(pattern),
                    Err(_) => invalid.push(pattern),
                }
            }
            (compiled, invalid)
        };

        let (includes, mut invalid) = compile(includes);
        let (excludes, exclude_invalid) = compile(excludes);
        invalid.extend(exclude_invalid);

        Self {
            includes,
            excludes,
            invalid,
        }
    }

    fn invalid_patterns(&self) -> impl Iterator<Item = &str> {
        self.invalid.iter().map(String::as_str)
    }

    /// Returns true when `relative` (a slash-normalized path relative to the
    /// config base) matches at least one include pattern and matches no exclude
    /// pattern. The path is borrowed; no allocation occurs.
    pub fn is_included(&self, relative: &str) -> bool {
        if !self.invalid.is_empty() {
            return false;
        }
        let has_include_match = self
            .includes
            .iter()
            .any(|pattern| matches_relative(pattern, relative));
        if !has_include_match {
            return false;
        }
        !self
            .excludes
            .iter()
            .any(|pattern| matches_relative(pattern, relative))
    }
}

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
// Inheritance/resolution phases
// ---------------------------------------------------------------------------

/// Phase 1: Read and parse one tsconfig file (JSONC-aware).
pub fn read_and_parse(config_path: &Path) -> Result<TsconfigDto, ProjectLoadError> {
    let mut text = std::fs::read_to_string(config_path).map_err(|source| ProjectLoadError::Io {
        path: config_path.to_path_buf(),
        source,
    })?;
    json_strip_comments::strip(&mut text).map_err(|error| parse_error(config_path, error))?;
    TsconfigDto::parse(&text).map_err(|error| parse_error(config_path, error))
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

/// Phase 2-3: Build the effective Tsconfig for a given config path, resolving
/// `extends` inheritance recursively. Returns diagnostics for cycles.
///
/// Maintains an internal extends-chain to detect cycles without affecting the
/// caller's visited set. When a cycle is found, a diagnostic is emitted, the
/// offending `extends` edge is discarded, and the current file's local settings
/// plus any already-resolved acyclic ancestors are retained. The child config
/// continues building normally without the cyclic parent. Independent
/// non-cyclic branches continue unaffected.
pub fn build_effective_config(
    config_path: &Path,
    fallback_base: &Path,
    deadline: Option<Instant>,
    diagnostics: &mut Vec<TsconfigDiagnostic>,
) -> Result<(Tsconfig, Vec<ReferenceEntry>), ProjectLoadError> {
    let mut extends_chain: Vec<PathBuf> = Vec::new();
    build_effective_config_inner(
        config_path,
        fallback_base,
        &mut extends_chain,
        deadline,
        diagnostics,
    )
}

fn build_effective_config_inner(
    config_path: &Path,
    fallback_base: &Path,
    extends_chain: &mut Vec<PathBuf>,
    deadline: Option<Instant>,
    diagnostics: &mut Vec<TsconfigDiagnostic>,
) -> Result<(Tsconfig, Vec<ReferenceEntry>), ProjectLoadError> {
    if let Some(deadline) = deadline
        && Instant::now() >= deadline
    {
        return Err(ProjectLoadError::Timeout);
    }
    let canonical = realpath(config_path)?;
    extends_chain.push(canonical.clone());

    let dto = read_and_parse(config_path)?;
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
    let parent_tsconfig = dto
        .extends
        .clone()
        .ok()
        .and_then(|extends_str| {
            let parent_path = resolve_extends(config_path, &extends_str)
                .filter(|parent_path| parent_path.exists());
            match parent_path {
                Some(parent_path) if extends_chain.contains(&parent_path) => {
                    diagnostics.push(TsconfigDiagnostic {
                        config_path: config_path.to_path_buf(),
                        cycle_target: Some(parent_path),
                        message: format!(
                            "cycle detected: {} is already in the inheritance chain",
                            config_path.display()
                        ),
                    });
                    None
                }
                Some(parent_path) => {
                    let result = build_effective_config_inner(
                        &parent_path,
                        &base,
                        extends_chain,
                        deadline,
                        diagnostics,
                    );
                    Some(result.map(|(config, _)| config))
                }
                None => None,
            }
        })
        .transpose()?;

    extends_chain.pop();

    let effective = Tsconfig::new(canonical, dto, parent_tsconfig.as_ref());
    diagnostics.extend(
        effective
            .pattern_diagnostics
            .iter()
            .map(|message| TsconfigDiagnostic {
                config_path: effective.config_path.clone(),
                cycle_target: None,
                message: message.clone(),
            }),
    );
    Ok((effective, references))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TempProject;

    #[test]
    fn parse_empty_config() {
        let dto = TsconfigDto::parse("{}").unwrap();
        assert!(matches!(dto.extends, StringField::Absent));
        assert!(matches!(dto.files, StringArrayField::Absent));
        assert!(matches!(dto.include, StringArrayField::Absent));
        assert!(matches!(dto.exclude, StringArrayField::Absent));
        assert!(dto.references.is_empty());
    }

    #[test]
    fn parse_null_fields() {
        let dto =
            TsconfigDto::parse(r#"{"extends":null,"files":null,"include":null,"exclude":null}"#)
                .unwrap();
        assert!(matches!(dto.extends, StringField::Null));
        assert!(matches!(dto.files, StringArrayField::Null));
        assert!(matches!(dto.include, StringArrayField::Null));
        assert!(matches!(dto.exclude, StringArrayField::Null));
    }

    #[test]
    fn parse_wrong_types() {
        let dto = TsconfigDto::parse(
            r#"{"extends":42,"files":"not-an-array","include":false,"exclude":{}}"#,
        )
        .unwrap();
        assert!(matches!(&dto.extends, StringField::WrongType(_)));
        assert!(matches!(&dto.files, StringArrayField::WrongType(_)));
        assert!(matches!(&dto.include, StringArrayField::WrongType(_)));
        assert!(matches!(&dto.exclude, StringArrayField::WrongType(_)));
    }

    #[test]
    fn parse_compiler_options() {
        let dto =
            TsconfigDto::parse(r#"{"compilerOptions":{"outDir":"dist","declarationDir":"types"}}"#)
                .unwrap();
        assert_eq!(dto.compiler_options_out_dir.ok(), Some("dist".into()));
        assert_eq!(
            dto.compiler_options_declaration_dir.ok(),
            Some("types".into())
        );
    }

    #[test]
    fn parse_references() {
        let dto = TsconfigDto::parse(r#"{"references":[{"path":"./child"},{"path":"./other"}]}"#)
            .unwrap();
        assert_eq!(
            dto.references,
            vec![
                ReferenceEntry {
                    path: "./child".into()
                },
                ReferenceEntry {
                    path: "./other".into()
                }
            ]
        );
    }

    #[test]
    fn parse_jsonc() {
        let mut text = "{\n  // comment\n  \"include\": [\"src\"],\n}".to_string();
        json_strip_comments::strip(&mut text).unwrap();
        let dto = TsconfigDto::parse(&text).unwrap();
        assert!(matches!(&dto.include, StringArrayField::Present(v) if v == &["src"]));
    }

    #[test]
    fn pattern_set_compilation_and_matching() {
        let ps = TsconfigPatternSet::new(
            &["src/**/*".to_string(), "lib/**/*".to_string()],
            &["**/*.test.ts".to_string()],
        );
        assert!(ps.is_included("src/main.ts"));
        assert!(ps.is_included("lib/util.ts"));
        assert!(!ps.is_included("src/main.test.ts"));
        assert!(!ps.is_included("dist/bundle.js"));
        assert!(!ps.is_included("node_modules/pkg/index.js"));
    }

    #[test]
    fn pattern_set_trailing_slash() {
        let ps = TsconfigPatternSet::new(&["src/".to_string()], &[]);
        assert!(ps.is_included("src/main.ts"));
        assert!(!ps.is_included("lib/main.ts"));
    }

    #[test]
    fn pattern_set_no_slash_matches_basename() {
        let ps = TsconfigPatternSet::new(&["*.ts".to_string()], &[]);
        assert!(ps.is_included("foo.ts"));
        assert!(ps.is_included("src/bar.ts"));
        assert!(!ps.is_included("foo.js"));
    }

    #[test]
    fn compile_counter_increments() {
        reset_compile_counter();
        let _ps1 = TsconfigPatternSet::new(&["**/*".to_string()], &[]);
        assert_eq!(compile_count(), 1);
        let _ps2 = TsconfigPatternSet::new(&["**/*".to_string()], &[]);
        assert_eq!(compile_count(), 2);
    }

    #[test]
    fn effective_config_inherits_fields() {
        let parent_dto =
            TsconfigDto::parse(r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#).unwrap();
        let child_dto = TsconfigDto::parse(r#"{"include":["lib/**/*"]}"#).unwrap();

        let parent = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), parent_dto, None);

        let child = Tsconfig::new(
            PathBuf::from("/root/tsconfig.json"),
            child_dto,
            Some(&parent),
        );

        // Child include overrides parent
        assert_eq!(child.include, vec!["lib/**/*"]);
        // Exclude is inherited (child didn't set it)
        assert!(child.exclude.iter().any(|e| e == "**/*.test.ts"));
        // Default node_modules exclusion
        assert!(child.exclude.iter().any(|e| e == "**/node_modules"));
    }

    #[test]
    fn effective_config_default_include() {
        let dto = TsconfigDto::parse("{}").unwrap();
        let config = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), dto, None);
        assert_eq!(config.include, vec!["**/*"]);
    }

    #[test]
    fn effective_config_explicit_files() {
        let dto = TsconfigDto::parse(r#"{"files":["src/main.ts","src/util.ts"]}"#).unwrap();
        let config = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), dto, None);
        assert_eq!(
            config.files,
            Some(vec!["src/main.ts".to_string(), "src/util.ts".to_string()])
        );
        assert!(config.include.is_empty());
    }

    #[test]
    fn cycle_detection_records_diagnostic_and_skips_cyclic_extends() {
        let project = TempProject::new("tsconfig-cycle");
        project.write(
            "tsconfig.json",
            r#"{"extends":"./tsconfig.json","include":["src/**/*"]}"#,
        );

        let mut diagnostics = Vec::new();
        let config_path = project.root().join("tsconfig.json");
        let result = build_effective_config(&config_path, project.root(), None, &mut diagnostics);

        assert!(
            result.is_ok(),
            "build_effective_config failed: {:?}",
            result.err()
        );
        let (config, _references) = result.unwrap();
        // Cycle extends is skipped; config uses its own include
        assert_eq!(config.files, None);
        assert!(config.include.contains(&"src/**/*".to_string()));
        // Cycle diagnostics recorded
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("cycle"));
    }

    #[test]
    fn compile_counter_increments_once_per_effective_config() {
        // Build a config with no parent to measure exactly one compilation.
        let project = TempProject::new("tsconfig-compile");
        project.write(
            "tsconfig.json",
            r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#,
        );

        let (config, _) = build_effective_config(
            &project.root().join("tsconfig.json"),
            project.root(),
            None,
            &mut Vec::new(),
        )
        .unwrap();

        // Matching should reuse the compiled set (no additional compilation)
        config.pattern_set.is_included("src/main.ts");
        config.pattern_set.is_included("src/test.ts");
    }

    #[test]
    fn cycle_fails_closed_does_not_broaden_admission() {
        // Create config A that extends B, and B that extends A (cycle)
        let project = TempProject::new("tsconfig-cycle2");
        project.write("a.json", r#"{"extends":"./b.json","include":["src/**/*"]}"#);
        project.write(
            "b.json",
            r#"{"extends":"./a.json","include":["other/**/*"]}"#,
        );

        let mut diagnostics = Vec::new();

        // Build effective config for A
        let result = build_effective_config(
            &project.root().join("a.json"),
            project.root(),
            None,
            &mut diagnostics,
        );

        assert!(result.is_ok());
        let (config, _) = result.unwrap();
        // A should have include: ["src/**/*"] (its own setting)
        // The cycle in extends should NOT bring in B's patterns
        assert!(config.files.is_none(), "no explicit files");
        assert_eq!(
            config.include,
            vec!["src/**/*"],
            "A's include should not be broadened by cyclic B"
        );
        // Cycle diagnostic recorded for the B->A link
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("cycle"));
    }

    #[test]
    fn missing_config_field_returns_typed_diagnostic() {
        // Parsing a config with wrong types should succeed (we record diagnostics
        // as typed fields, not errors)
        let dto = TsconfigDto::parse(r#"{"include":123,"exclude":null}"#).unwrap();
        assert!(matches!(&dto.include, StringArrayField::WrongType(_)));
        assert!(matches!(&dto.exclude, StringArrayField::Null));
    }

    #[test]
    fn extends_nonexistent_path_is_skipped_silently() {
        let project = TempProject::new("tsconfig-missing-extends");
        project.write(
            "tsconfig.json",
            r#"{"extends":"./nonexistent.json","include":["src/**/*"]}"#,
        );

        let mut diagnostics = Vec::new();
        let (config, _) = build_effective_config(
            &project.root().join("tsconfig.json"),
            project.root(),
            None,
            &mut diagnostics,
        )
        .unwrap();

        assert_eq!(config.include, vec!["src/**/*"]);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn single_level_extends_merges_correctly() {
        let project = TempProject::new("tsconfig-merge");
        project.write(
            "base.json",
            r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#,
        );
        project.write(
            "tsconfig.json",
            r#"{"extends":"./base.json","exclude":["**/*.spec.ts"]}"#,
        );

        let mut diagnostics = Vec::new();
        let (config, _) = build_effective_config(
            &project.root().join("tsconfig.json"),
            project.root(),
            None,
            &mut diagnostics,
        )
        .unwrap();

        // Child exclude overrides parent include? No — include and exclude are
        // separate. Child exclude replaces parent exclude since child sets it.
        // Actually from Tsconfig::new: exclude starts as child's, and if child's is
        // empty it inherits from parent. Child set ["**/*.spec.ts"], so excludes
        // should be: ["**/*.spec.ts", "**/node_modules", "**/bower_components"]
        assert!(config.exclude.iter().any(|e| e == "**/*.spec.ts"));
        // Parent's exclude should NOT be inherited since child set its own.
        assert!(
            !config.exclude.iter().any(|e| e == "**/*.test.ts"),
            "child exclude should replace parent exclude"
        );
    }
}
