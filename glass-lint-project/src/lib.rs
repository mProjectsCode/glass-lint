//! Bounded filesystem construction for Glass Lint projects.
//!
//! This crate owns discovery, source loading, project-boundary checks, and
//! Oxc resolution. Core receives only owned sources and typed resolution
//! results; no resolver or filesystem type crosses that boundary.

use glass_lint_core::{
    Linter, ProjectInputError, ProjectReport, ResolutionRequest, ResolutionRequestKind,
    ResolutionResult, SourceFile, SourceLanguage,
};
use oxc_resolver::{ResolveOptions, Resolver};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::{DirEntry, WalkDir};

const DEFAULT_MAX_FILES: usize = 10_000;
const DEFAULT_MAX_REQUESTS: usize = 50_000;
const DEFAULT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_EXTENSIONS: &[&str] = &[".js", ".cjs", ".mjs", ".ts", ".cts", ".mts"];

/// How a filesystem project is selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectSelection {
    Entry(PathBuf),
    Directory(PathBuf),
    TsConfig(PathBuf),
}

impl ProjectSelection {
    pub fn entry(path: impl Into<PathBuf>) -> Self {
        Self::Entry(path.into())
    }
    pub fn directory(path: impl Into<PathBuf>) -> Self {
        Self::Directory(path.into())
    }
    pub fn tsconfig(path: impl Into<PathBuf>) -> Self {
        Self::TsConfig(path.into())
    }
    fn path(&self) -> &Path {
        match self {
            Self::Entry(path) | Self::Directory(path) | Self::TsConfig(path) => path,
        }
    }
}

/// Validated policy for filesystem project loading.
#[derive(Clone, Debug)]
pub struct ProjectLoadOptions {
    /// Project boundary. If omitted, the selection's directory is used.
    pub root: Option<PathBuf>,
    pub max_files: usize,
    pub max_requests: usize,
    pub max_source_bytes: u64,
    pub extensions: Vec<String>,
    pub excluded_directories: BTreeSet<String>,
    pub follow_symlinks: bool,
    pub extension_aliases: BTreeMap<String, Vec<String>>,
}

impl Default for ProjectLoadOptions {
    fn default() -> Self {
        Self {
            root: None,
            max_files: DEFAULT_MAX_FILES,
            max_requests: DEFAULT_MAX_REQUESTS,
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            extensions: DEFAULT_EXTENSIONS.iter().map(|s| (*s).to_owned()).collect(),
            excluded_directories: [".git", "node_modules", "dist", "build", "target"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            follow_symlinks: false,
            extension_aliases: BTreeMap::new(),
        }
    }
}

impl ProjectLoadOptions {
    pub fn validate(&self) -> Result<(), ProjectLoadError> {
        if self.max_files == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                "max_files must be positive".into(),
            ));
        }
        if self.max_requests == 0 {
            return Err(ProjectLoadError::InvalidOptions(
                "max_requests must be positive".into(),
            ));
        }
        if self.max_source_bytes == 0
            || self.max_source_bytes > glass_lint_core::MAX_SOURCE_BYTES as u64
        {
            return Err(ProjectLoadError::InvalidOptions(format!(
                "max_source_bytes must be between 1 and {}",
                glass_lint_core::MAX_SOURCE_BYTES
            )));
        }
        if self.extensions.is_empty()
            || self
                .extensions
                .iter()
                .any(|extension| !valid_extension(extension))
        {
            return Err(ProjectLoadError::InvalidOptions(
                "extensions must be non-empty file suffixes".into(),
            ));
        }
        if self.extension_aliases.iter().any(|(extension, aliases)| {
            !valid_extension(extension)
                || aliases.is_empty()
                || aliases.iter().any(|alias| !valid_extension(alias))
        }) {
            return Err(ProjectLoadError::InvalidOptions(
                "extension aliases must map file suffixes to non-empty suffix lists".into(),
            ));
        }
        Ok(())
    }
}

/// Operational and semantic errors from project construction.
#[derive(Debug)]
pub enum ProjectLoadError {
    InvalidOptions(String),
    SelectionNotFound(PathBuf),
    SelectionNotFile(PathBuf),
    SelectionOutsideRoot {
        selection: PathBuf,
        root: PathBuf,
    },
    UnsupportedSource(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    TooManyFiles(usize),
    TooManyRequests(usize),
    SourceTooLarge {
        path: PathBuf,
        bytes: u64,
        limit: u64,
    },
    Core(ProjectInputError),
}

impl fmt::Display for ProjectLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(f, "invalid project options: {message}"),
            Self::SelectionNotFound(path) => {
                write!(f, "project selection does not exist: {}", path.display())
            }
            Self::SelectionNotFile(path) => {
                write!(f, "project entry is not a file: {}", path.display())
            }
            Self::SelectionOutsideRoot { selection, root } => write!(
                f,
                "project selection {} is outside project root {}",
                selection.display(),
                root.display()
            ),
            Self::UnsupportedSource(path) => {
                write!(f, "unsupported project source: {}", path.display())
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::TooManyFiles(limit) => write!(f, "project file limit exceeded ({limit})"),
            Self::TooManyRequests(limit) => {
                write!(f, "project resolution request limit exceeded ({limit})")
            }
            Self::SourceTooLarge { path, bytes, limit } => {
                write!(f, "{} is {bytes} bytes, exceeding {limit}", path.display())
            }
            Self::Core(error) => write!(f, "core project error: {error}"),
        }
    }
}
impl std::error::Error for ProjectLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ProjectInputError> for ProjectLoadError {
    fn from(error: ProjectInputError) -> Self {
        Self::Core(error)
    }
}

/// Filesystem loader and Oxc resolver configuration.
#[derive(Clone, Debug)]
pub struct ProjectLoader {
    options: ProjectLoadOptions,
}

/// Bounded construction counters and phase timings for profiling. The
/// timings intentionally stop at the core boundary; matcher work is included
/// in `linking_and_matching` because core owns the completed project pass.
#[derive(Clone, Debug, Default)]
pub struct ProjectLoadMetrics {
    pub discovery: Duration,
    pub reads: Duration,
    pub parse_and_local_analysis: Duration,
    pub resolution: Duration,
    pub linking_and_matching: Duration,
    pub linking: Duration,
    pub matching: Duration,
    pub total: Duration,
    pub files: usize,
    pub requests: usize,
    pub edges: usize,
    pub bytes: u64,
}

impl ProjectLoader {
    pub fn new(options: ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        options.validate()?;
        Ok(Self { options })
    }
    pub fn options(&self) -> &ProjectLoadOptions {
        &self.options
    }

    /// Loads, resolves, and lints one bounded project.
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_and_lint(
        &self,
        linter: &Linter,
        selection: ProjectSelection,
    ) -> Result<ProjectReport, ProjectLoadError> {
        Ok(self.load_and_lint_with_metrics(linter, selection)?.0)
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn load_and_lint_with_metrics(
        &self,
        linter: &Linter,
        selection: ProjectSelection,
    ) -> Result<(ProjectReport, ProjectLoadMetrics), ProjectLoadError> {
        let mut metrics = ProjectLoadMetrics::default();
        let total_start = Instant::now();
        let report = self.load_and_lint_inner(linter, &selection, &mut metrics)?;
        metrics.total = total_start.elapsed();
        Ok((report, metrics))
    }

    fn load_and_lint_inner(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<ProjectReport, ProjectLoadError> {
        let selection_path = absolute_path(selection.path());
        if !selection_path.exists() {
            return Err(ProjectLoadError::SelectionNotFound(selection_path));
        }
        let selection_path = realpath(&selection_path)?;
        let root = realpath(&self.project_root(selection, &selection_path))?;
        if !inside_root(&root, &selection_path) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: selection_path,
                root,
            });
        }
        let import_resolver = Resolver::new(self.resolver_options(&root, selection, false));
        let require_resolver =
            import_resolver.clone_with_options(self.resolver_options(&root, selection, true));
        let mut session = linter.begin_project(&root)?;
        let discovery_start = Instant::now();
        let mut queued = self.initial_paths(selection, &selection_path, &root)?;
        metrics.discovery += discovery_start.elapsed();
        let mut admitted = BTreeSet::new();
        let mut request_count = 0usize;
        let mut resolved: BTreeMap<(String, ResolutionRequestKind, String), ResolutionResult> =
            BTreeMap::new();

        while let Some(path) = queued.pop_front() {
            let path = realpath(&path)?;
            if !inside_root(&root, &path) || !admitted.insert(path.clone()) {
                continue;
            }
            if admitted.len() > self.options.max_files {
                return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
            }
            let read_start = Instant::now();
            let source = self.read_source(&root, &path)?;
            metrics.reads += read_start.elapsed();
            let source_bytes = u64::try_from(source.source.len()).unwrap_or(u64::MAX);
            let parse_start = Instant::now();
            let requests = session.add_source(source)?;
            metrics.parse_and_local_analysis += parse_start.elapsed();
            metrics.bytes = metrics.bytes.saturating_add(source_bytes);
            metrics.files = admitted.len();
            request_count = request_count.saturating_add(requests.len());
            metrics.requests = request_count;
            if request_count > self.options.max_requests {
                return Err(ProjectLoadError::TooManyRequests(self.options.max_requests));
            }
            for request in requests {
                let cache_key = (
                    request.key.importer.clone(),
                    request.key.kind,
                    request.request.clone(),
                );
                let result = if let Some(result) = resolved.get(&cache_key) {
                    result.clone()
                } else {
                    let resolve_start = Instant::now();
                    let result =
                        self.resolve_request(&import_resolver, &require_resolver, &root, &request);
                    metrics.resolution += resolve_start.elapsed();
                    resolved.insert(cache_key, result.clone());
                    result
                };
                if let ResolutionResult::Internal { path } = &result {
                    metrics.edges = metrics.edges.saturating_add(1);
                    let target = root.join(path);
                    if target.exists()
                        && !excluded_path(&root, &target, &self.options.excluded_directories)
                        && supported_path(&target, &self.options.extensions)
                    {
                        queued.push_back(target);
                    }
                }
                session.record_resolution(request.key, result)?;
            }
        }
        let link_start = Instant::now();
        let (report, linking, matching) = session.finish_with_timings()?;
        metrics.linking += linking;
        metrics.matching += matching;
        metrics.linking_and_matching += link_start.elapsed();
        Ok(report)
    }

    fn project_root(&self, selection: &ProjectSelection, path: &Path) -> PathBuf {
        if let Some(root) = &self.options.root {
            return absolute_path(root);
        }
        match selection {
            ProjectSelection::Directory(_) => path.to_path_buf(),
            ProjectSelection::Entry(_) | ProjectSelection::TsConfig(_) => {
                path.parent().unwrap_or(path).to_path_buf()
            }
        }
    }
    fn initial_paths(
        &self,
        selection: &ProjectSelection,
        path: &Path,
        root: &Path,
    ) -> Result<VecDeque<PathBuf>, ProjectLoadError> {
        let mut paths = match selection {
            ProjectSelection::Entry(_) => {
                if !path.is_file() {
                    return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
                }
                if !supported_path(path, &self.options.extensions) {
                    return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
                }
                vec![path.to_path_buf()]
            }
            ProjectSelection::Directory(_) => self.discover(path)?,
            ProjectSelection::TsConfig(config) => {
                if !path.is_file() {
                    return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
                }
                self.discover_tsconfig(config, path.parent().unwrap_or(root))?
            }
        };
        if paths.iter().any(|path| !inside_root(root, path)) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: path.to_path_buf(),
                root: root.to_path_buf(),
            });
        }
        paths.retain(|path| inside_root(root, path));
        paths.sort();
        paths.dedup();
        if paths.len() > self.options.max_files {
            return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
        }
        Ok(paths.into())
    }
    fn discover(&self, directory: &Path) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut entries = Vec::new();
        let walker = WalkDir::new(directory)
            .follow_links(self.options.follow_symlinks)
            .sort_by_file_name();
        for entry in walker
            .into_iter()
            .filter_entry(|entry| self.include_dir(entry))
        {
            let entry = entry.map_err(|error| ProjectLoadError::Io {
                path: error.path().unwrap_or(directory).to_path_buf(),
                source: error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("directory traversal failed")),
            })?;
            if entry.file_type().is_file() && supported_path(entry.path(), &self.options.extensions)
            {
                entries.push(entry.into_path());
            }
        }
        Ok(entries)
    }
    fn discover_tsconfig(
        &self,
        config: &Path,
        directory: &Path,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut visited = BTreeSet::new();
        let mut selected = BTreeSet::new();
        self.collect_tsconfig(&realpath(config)?, directory, &mut visited, &mut selected)?;
        Ok(selected.into_iter().collect())
    }

    fn collect_tsconfig(
        &self,
        config: &Path,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
        selected: &mut BTreeSet<PathBuf>,
    ) -> Result<(), ProjectLoadError> {
        let config = realpath(config)?;
        if !visited.insert(config.clone()) {
            return Ok(());
        }
        let parsed = self.read_tsconfig_with_extends(&config, fallback_directory, visited)?;
        let base = config.parent().unwrap_or(fallback_directory);
        let all = self.discover(base)?;
        let includes = parsed
            .get("include")
            .and_then(serde_json::Value::as_array)
            .map_or_else(
                || vec!["**/*"],
                |values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                },
            );
        let mut excludes = parsed
            .get("exclude")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        excludes.extend(["**/node_modules", "**/bower_components"]);
        if let Some(options) = parsed.get("compilerOptions") {
            for option in ["outDir", "declarationDir"] {
                if let Some(directory) = options.get(option).and_then(serde_json::Value::as_str) {
                    excludes.push(directory);
                }
            }
        }
        if let Some(files) = parsed.get("files").and_then(serde_json::Value::as_array) {
            for file in files.iter().filter_map(serde_json::Value::as_str) {
                let path = base.join(file);
                if path.exists() && supported_path(&path, &self.options.extensions) {
                    selected.insert(realpath(&path)?);
                }
            }
        } else {
            for path in all {
                let relative = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if includes
                    .iter()
                    .any(|pattern| tsconfig_pattern_matches(pattern, &relative))
                    && !excludes
                        .iter()
                        .any(|pattern| tsconfig_pattern_matches(pattern, &relative))
                {
                    selected.insert(realpath(&path)?);
                }
            }
        }
        if let Some(references) = parsed
            .get("references")
            .and_then(serde_json::Value::as_array)
        {
            for reference in references {
                let Some(path) = reference.get("path").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let mut target = base.join(path);
                if target.is_dir() {
                    target = target.join("tsconfig.json");
                }
                if target.exists() {
                    self.collect_tsconfig(&target, base, visited, selected)?;
                }
            }
        }
        Ok(())
    }

    /// Materialize inherited options before selecting runtime sources. A child
    /// config overrides scalar/array fields, while object fields such as
    /// `compilerOptions` are merged recursively.
    #[allow(clippy::self_only_used_in_recursion)]
    fn read_tsconfig_with_extends(
        &self,
        config: &Path,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
    ) -> Result<serde_json::Value, ProjectLoadError> {
        let mut text = fs::read_to_string(config).map_err(|source| ProjectLoadError::Io {
            path: config.to_path_buf(),
            source,
        })?;
        json_strip_comments::strip(&mut text).map_err(|error| {
            ProjectLoadError::InvalidOptions(format!("parse {}: {error}", config.display()))
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
            ProjectLoadError::InvalidOptions(format!("parse {}: {error}", config.display()))
        })?;
        let mut effective = serde_json::Value::Object(serde_json::Map::new());
        if let Some(extends) = parsed.get("extends").and_then(serde_json::Value::as_str)
            && let Some(parent) = resolve_tsconfig_extends(config, extends, fallback_directory)
            && parent.exists()
        {
            let parent = realpath(&parent)?;
            if visited.insert(parent.clone()) {
                effective = self.read_tsconfig_with_extends(
                    &parent,
                    parent.parent().unwrap_or(fallback_directory),
                    visited,
                )?;
            }
        }
        merge_tsconfig_json(&mut effective, parsed);
        Ok(effective)
    }
    fn include_dir(&self, entry: &DirEntry) -> bool {
        !entry.file_type().is_dir()
            || entry
                .file_name()
                .to_str()
                .is_none_or(|name| !self.options.excluded_directories.contains(name))
    }
    fn read_source(&self, root: &Path, path: &Path) -> Result<SourceFile, ProjectLoadError> {
        if !supported_path(path, &self.options.extensions) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        let metadata = fs::metadata(path).map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if metadata.len() > self.options.max_source_bytes {
            return Err(ProjectLoadError::SourceTooLarge {
                path: path.to_path_buf(),
                bytes: metadata.len(),
                limit: self.options.max_source_bytes,
            });
        }
        let source = fs::read_to_string(path).map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(SourceFile {
            language: SourceLanguage::from_filename(&relative),
            path: relative,
            source,
        })
    }
    fn resolver_options(
        &self,
        root: &Path,
        selection: &ProjectSelection,
        require: bool,
    ) -> ResolveOptions {
        let extension_alias = self
            .options
            .extension_aliases
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut options = ResolveOptions {
            condition_names: if require {
                vec!["node".into(), "require".into()]
            } else {
                vec!["node".into(), "import".into()]
            },
            extensions: self.options.extensions.clone(),
            extension_alias,
            symlinks: self.options.follow_symlinks,
            roots: vec![root.to_path_buf()],
            ..ResolveOptions::default()
        };
        if let ProjectSelection::TsConfig(path) = selection {
            options.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Manual(
                oxc_resolver::TsconfigOptions {
                    config_file: absolute_path(path),
                    references: oxc_resolver::TsconfigReferences::Auto,
                },
            ));
        }
        options
    }
    fn resolve_request(
        &self,
        import_resolver: &Resolver,
        require_resolver: &Resolver,
        root: &Path,
        request: &ResolutionRequest,
    ) -> ResolutionResult {
        if is_builtin(&request.request) {
            return ResolutionResult::Builtin {
                name: request.request.clone(),
            };
        }
        let importer = root.join(&request.key.importer);
        let directory = importer.parent().unwrap_or(root);
        let resolver = if request.key.kind == ResolutionRequestKind::Require {
            require_resolver
        } else {
            import_resolver
        };
        let result = resolver.resolve(directory, &request.request);
        match result {
            Ok(resolution) => self.classify_resolution(root, &request.request, resolution.path()),
            Err(_) if is_internal_request(&request.request) => ResolutionResult::Missing,
            Err(_) => ResolutionResult::External {
                package: package_name(&request.request),
            },
        }
    }
    fn classify_resolution(&self, root: &Path, request: &str, path: &Path) -> ResolutionResult {
        let Ok(path) = realpath(path) else {
            return ResolutionResult::Missing;
        };
        if !inside_root(root, &path) {
            return if is_internal_request(request) {
                ResolutionResult::OutsideProject {
                    path: path.to_string_lossy().into_owned(),
                }
            } else {
                ResolutionResult::External {
                    package: package_name(request),
                }
            };
        }
        if excluded_path(root, &path, &self.options.excluded_directories) {
            if !is_internal_request(request) {
                return ResolutionResult::External {
                    package: package_name(request),
                };
            }
            return ResolutionResult::Unsupported {
                reason: format!("excluded target `{}`", path.display()),
            };
        }
        if !supported_path(&path, &self.options.extensions) {
            return ResolutionResult::Unsupported {
                reason: format!("unsupported target `{}`", path.display()),
            };
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        ResolutionResult::Internal { path: relative }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}
fn realpath(path: &Path) -> Result<PathBuf, ProjectLoadError> {
    fs::canonicalize(path).map_err(|source| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}
fn inside_root(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}
fn excluded_path(root: &Path, path: &Path, excluded: &BTreeSet<String>) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| excluded.contains(name))
        })
    })
}
fn supported_path(path: &Path, extensions: &[String]) -> bool {
    let name = path.to_string_lossy().to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| name.ends_with(&extension.to_ascii_lowercase()))
        && ![".d.ts", ".d.cts", ".d.mts"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}
fn valid_extension(extension: &str) -> bool {
    extension.len() >= 2
        && extension.starts_with('.')
        && !extension
            .chars()
            .any(|character| character == '/' || character == '\\' || character == '\0')
}
fn tsconfig_pattern_matches(pattern: &str, relative: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let pattern = if pattern.ends_with('/') {
        format!("{pattern}**/*")
    } else {
        pattern
    };
    glob::Pattern::new(&pattern).is_ok_and(|pattern| {
        pattern.matches(relative)
            || (!pattern.as_str().contains('/')
                && relative
                    .split('/')
                    .next_back()
                    .is_some_and(|name| pattern.matches(name)))
    })
}

fn resolve_tsconfig_extends(
    config: &Path,
    extends: &str,
    fallback_directory: &Path,
) -> Option<PathBuf> {
    // Package-based `extends` is resolver policy rather than source
    // membership. Relative and absolute configs cover the project-boundary
    // contract without accidentally admitting a dependency's sources.
    if !extends.starts_with('.') && !Path::new(extends).is_absolute() {
        return None;
    }
    let base = config.parent().unwrap_or(fallback_directory);
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

fn merge_tsconfig_json(base: &mut serde_json::Value, child: serde_json::Value) {
    match (base, child) {
        (serde_json::Value::Object(base), serde_json::Value::Object(child)) => {
            for (key, value) in child {
                if let Some(existing) = base.get_mut(&key) {
                    if key == "compilerOptions" {
                        merge_tsconfig_json(existing, value);
                    } else {
                        *existing = value;
                    }
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, child) => *base = child,
    }
}

fn is_internal_request(request: &str) -> bool {
    request.starts_with('.') || request.starts_with('/') || request.starts_with('#')
}
fn package_name(request: &str) -> String {
    if request.starts_with('@') {
        request.split('/').take(2).collect::<Vec<_>>().join("/")
    } else {
        request.split('/').next().unwrap_or(request).to_owned()
    }
}
fn is_builtin(request: &str) -> bool {
    request.starts_with("node:")
        || matches!(
            request,
            "assert"
                | "buffer"
                | "child_process"
                | "crypto"
                | "events"
                | "fs"
                | "http"
                | "https"
                | "module"
                | "net"
                | "os"
                | "path"
                | "perf_hooks"
                | "process"
                | "stream"
                | "string_decoder"
                | "timers"
                | "tls"
                | "tty"
                | "url"
                | "util"
                | "v8"
                | "vm"
                | "worker_threads"
                | "zlib"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_lint_core::{Environment, Linter, RuleCatalog};
    fn linter() -> Linter {
        Linter::new(RuleCatalog::with_environment("test", vec![], Environment::default()).unwrap())
    }
    #[test]
    fn directory_discovery_is_sorted_and_excludes_runtime_directories() {
        let root = std::env::temp_dir().join(format!("glass-lint-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("src/z.ts"), "").unwrap();
        fs::write(root.join("src/a.js"), "").unwrap();
        fs::write(root.join("src/types.d.ts"), "").unwrap();
        fs::write(root.join("src/types.d.cts"), "").unwrap();
        fs::write(root.join("src/types.d.mts"), "").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "").unwrap();
        let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
        let report = loader
            .load_and_lint(&linter(), ProjectSelection::directory(&root))
            .unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["src/a.js", "src/z.ts"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolver_suffix_options_are_validated_and_declarations_are_excluded() {
        let mut options = ProjectLoadOptions::default();
        options.extension_aliases.insert(".js".into(), vec![]);
        assert!(matches!(
            ProjectLoader::new(options),
            Err(ProjectLoadError::InvalidOptions(_))
        ));

        let mut options = ProjectLoadOptions::default();
        options.extensions.push(".d.cts".into());
        assert!(ProjectLoader::new(options).is_ok());
    }
    #[test]
    fn extensionless_internal_import_is_followed() {
        let root =
            std::env::temp_dir().join(format!("glass-lint-project-ext-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.js"), "import './helper';").unwrap();
        fs::write(root.join("helper.ts"), "export const value = 1;").unwrap();
        let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
        let report = loader
            .load_and_lint(&linter(), ProjectSelection::entry(root.join("main.js")))
            .unwrap();
        assert_eq!(report.files.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_project_phase_metrics_and_operation_counts() {
        let root =
            std::env::temp_dir().join(format!("glass-lint-project-metrics-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.js"), "import './helper';").unwrap();
        fs::write(root.join("helper.ts"), "export const value = 1;").unwrap();
        let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
        let (report, metrics) = loader
            .load_and_lint_with_metrics(&linter(), ProjectSelection::entry(root.join("main.js")))
            .unwrap();
        assert_eq!(report.files.len(), 2);
        assert_eq!(metrics.files, 2);
        assert_eq!(metrics.requests, 1);
        assert_eq!(metrics.edges, 1);
        assert!(metrics.total >= metrics.linking_and_matching);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tsconfig_membership_accepts_jsonc_and_excludes_files() {
        let root = std::env::temp_dir().join(format!(
            "glass-lint-project-tsconfig-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.ts"), "export const main = 1;").unwrap();
        fs::write(root.join("src/test.ts"), "export const test = 1;").unwrap();
        fs::write(
            root.join("tsconfig.json"),
            "{\n  // runtime project\n  \"include\": [\"src/**/*.ts\",],\n  \"exclude\": [\"src/test.ts\",],\n}",
        )
        .unwrap();
        let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
        let report = loader
            .load_and_lint(
                &linter(),
                ProjectSelection::tsconfig(root.join("tsconfig.json")),
            )
            .unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["src/main.ts"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tsconfig_membership_inherits_extends_and_collects_references() {
        let root = std::env::temp_dir().join(format!(
            "glass-lint-project-tsconfig-inherited-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("generated")).unwrap();
        fs::create_dir_all(root.join("packages/child/src")).unwrap();
        fs::write(root.join("src/main.ts"), "export const main = 1;").unwrap();
        fs::write(
            root.join("generated/main.ts"),
            "export const generated = 1;",
        )
        .unwrap();
        fs::write(
            root.join("packages/child/src/value.ts"),
            "export const value = 1;",
        )
        .unwrap();
        fs::write(
            root.join("base.json"),
            "{\"include\":[\"src/**/*.ts\"],\"compilerOptions\":{\"outDir\":\"generated\"}}",
        )
        .unwrap();
        fs::write(
            root.join("packages/child/tsconfig.json"),
            "{\"include\":[\"src/**/*.ts\"]}",
        )
        .unwrap();
        fs::write(
            root.join("tsconfig.json"),
            "{\"extends\":\"./base.json\",\"references\":[{\"path\":\"packages/child\"}]}",
        )
        .unwrap();

        let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
        let report = loader
            .load_and_lint(
                &linter(),
                ProjectSelection::tsconfig(root.join("tsconfig.json")),
            )
            .unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["packages/child/src/value.ts", "src/main.ts"]
        );
        fs::remove_dir_all(root).unwrap();
    }
}
