//! Validated, filesystem-free contracts for project-level analysis.

use crate::{SourceLanguage, SourceRange};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct SourceFile {
    pub path: String,
    pub language: SourceLanguage,
    pub source: String,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ResolutionRequestKind {
    Import,
    DynamicImport,
    Require,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequestKey {
    pub importer: String,
    pub kind: ResolutionRequestKind,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ResolutionRequest {
    pub key: ResolutionRequestKey,
    pub request: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolutionResult {
    Internal { path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct ModuleId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum ResolvedModule {
    Internal { id: ModuleId, path: String },
    External { package: String },
    Builtin { name: String },
    Missing,
    OutsideProject { path: String },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SourceLocation {
    pub path: String,
    pub range: SourceRange,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectEvidence {
    pub message: String,
    pub location: Option<SourceLocation>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFinding {
    pub rule_id: crate::RuleId,
    pub message_id: String,
    pub message: String,
    pub severity: crate::Severity,
    pub location: SourceLocation,
    pub evidence: Vec<ProjectEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectFileReport {
    pub path: String,
    pub findings: Vec<ProjectFinding>,
    pub parse_diagnostics: Vec<crate::ParseDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectDiagnostic {
    pub code: String,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub files: Vec<ProjectFileReport>,
    pub diagnostics: Vec<ProjectDiagnostic>,
    pub operations: ProjectOperationCounts,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectOperationCounts {
    pub files: usize,
    pub requests: usize,
    pub edges: usize,
    pub exports: usize,
    pub scc_rounds: usize,
    pub effect_projections: usize,
    pub evidence: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProjectInput {
    pub root: PathBuf,
    pub sources: Vec<SourceFile>,
    pub resolutions: Vec<(ResolutionRequestKey, ResolutionResult)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInputError {
    InvalidPath(String),
    DuplicateSource(String),
    UnknownImporter(String),
    InvalidRange(String),
    DuplicateResolution(ResolutionRequestKey),
    InvalidTarget(String),
    UnknownRequest(ResolutionRequestKey),
    BudgetExceeded(String),
}
impl fmt::Display for ProjectInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(path) => write!(f, "invalid project path `{path}`"),
            Self::DuplicateSource(path) => write!(f, "duplicate project source `{path}`"),
            Self::UnknownImporter(path) => {
                write!(f, "resolution importer is not a source: `{path}`")
            }
            Self::InvalidRange(path) => write!(f, "resolution range is invalid for `{path}`"),
            Self::DuplicateResolution(key) => {
                write!(f, "duplicate resolution for `{}`", key.importer)
            }
            Self::InvalidTarget(path) => write!(f, "invalid resolution target `{path}`"),
            Self::UnknownRequest(key) => write!(
                f,
                "resolution does not match an authored request in `{}`",
                key.importer
            ),
            Self::BudgetExceeded(message) => write!(f, "project input budget exceeded: {message}"),
        }
    }
}
impl std::error::Error for ProjectInputError {}

impl ProjectInput {
    /// Canonicalizes project identities and validates all cross-record references.
    pub fn validate(mut self) -> Result<Self, ProjectInputError> {
        if self.sources.len() > 100_000 {
            return Err(ProjectInputError::BudgetExceeded("source count".into()));
        }
        if self.resolutions.len() > 500_000 {
            return Err(ProjectInputError::BudgetExceeded("resolution count".into()));
        }
        if self
            .sources
            .iter()
            .map(|source| source.source.len())
            .sum::<usize>()
            > 512 * 1024 * 1024
        {
            return Err(ProjectInputError::BudgetExceeded(
                "project source bytes".into(),
            ));
        }
        self.root = normalize_root(&self.root)?;
        let mut sources = BTreeMap::new();
        for mut source in self.sources {
            source.path = normalize_relative(&source.path)?;
            let path = source.path.clone();
            if sources.insert(path.clone(), source).is_some() {
                return Err(ProjectInputError::DuplicateSource(path));
            }
        }
        let source_paths = sources
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut resolutions = BTreeMap::new();
        for (mut key, mut result) in self.resolutions {
            key.importer = normalize_relative(&key.importer)?;
            if !source_paths.contains(&key.importer) {
                return Err(ProjectInputError::UnknownImporter(key.importer));
            }
            if key.range.start.line == 0
                || key.range.start.column == 0
                || key.range.end.line == 0
                || key.range.end.column == 0
                || key.range.end.line < key.range.start.line
                || (key.range.end.line == key.range.start.line
                    && key.range.end.column < key.range.start.column)
            {
                return Err(ProjectInputError::InvalidRange(key.importer));
            }
            normalize_result(&mut result)?;
            if resolutions.insert(key.clone(), result).is_some() {
                return Err(ProjectInputError::DuplicateResolution(key));
            }
        }
        self.sources = sources.into_values().collect();
        self.resolutions = resolutions.into_iter().collect();
        Ok(self)
    }

    /// Assigns stable IDs from normalized project-relative paths.
    #[must_use]
    pub(crate) fn module_ids(&self) -> BTreeMap<String, ModuleId> {
        let mut paths = self
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                (
                    path,
                    ModuleId(u32::try_from(index).expect("module count exceeds ModuleId range")),
                )
            })
            .collect()
    }
}

/// A staged project collection session. Sources are parsed and locally
/// analyzed when added; `finish` links the retained models after all resolver
/// answers have been recorded.
pub struct ProjectSession<'a> {
    linter: &'a crate::Linter,
    root: PathBuf,
    sources: BTreeMap<String, SourceFile>,
    resolutions: BTreeMap<ResolutionRequestKey, ResolutionResult>,
    authored_requests: BTreeMap<ResolutionRequestKey, ResolutionRequest>,
    analyzed: BTreeMap<
        String,
        (
            swc_common::sync::Lrc<swc_common::SourceMap>,
            crate::analysis::LocalModuleModel,
        ),
    >,
    parse_diagnostics: BTreeMap<String, crate::ParseDiagnostic>,
}

impl<'a> ProjectSession<'a> {
    pub(crate) fn new(
        linter: &'a crate::Linter,
        root: impl Into<PathBuf>,
    ) -> Result<Self, ProjectInputError> {
        Ok(Self {
            linter,
            root: normalize_root(&root.into())?,
            sources: BTreeMap::new(),
            resolutions: BTreeMap::new(),
            authored_requests: BTreeMap::new(),
            analyzed: BTreeMap::new(),
            parse_diagnostics: BTreeMap::new(),
        })
    }

    /// Adds and locally analyzes a source once, returning only the semantic-free
    /// requests a filesystem resolver needs to answer.
    pub fn add_source(
        &mut self,
        mut source: SourceFile,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        source.path = normalize_relative(&source.path)?;
        let path = source.path.clone();
        if self.sources.contains_key(&path) {
            return Err(ProjectInputError::DuplicateSource(path));
        }
        self.sources.insert(path.clone(), source);
        let source = self.sources.get(&path).expect("source was just inserted");
        match crate::parse::parse_with_language(&source.source, &source.path, source.language) {
            Ok(parsed) => {
                let local = crate::analysis::LocalModuleModel::analyze(
                    &parsed.program,
                    self.linter.analysis_environment(),
                );
                let requests = local
                    .interface()
                    .authored_requests(&path, &parsed.source_map);
                for request in &requests {
                    self.authored_requests
                        .insert(request.key.clone(), request.clone());
                }
                self.analyzed.insert(path, (parsed.source_map, local));
                Ok(requests)
            }
            Err(error) => {
                self.parse_diagnostics.insert(path, error);
                Ok(Vec::new())
            }
        }
    }

    pub fn record_resolution(
        &mut self,
        key: ResolutionRequestKey,
        mut result: ResolutionResult,
    ) -> Result<(), ProjectInputError> {
        let key = ProjectInput {
            root: self.root.clone(),
            sources: self.sources.values().cloned().collect(),
            resolutions: vec![(key, result.clone())],
        }
        .validate()?
        .resolutions
        .into_iter()
        .next()
        .expect("one resolution")
        .0;
        if !self.authored_requests.contains_key(&key) {
            return Err(ProjectInputError::UnknownRequest(key));
        }
        normalize_result(&mut result)?;
        if self.resolutions.insert(key.clone(), result).is_some() {
            return Err(ProjectInputError::DuplicateResolution(key));
        }
        Ok(())
    }

    pub fn finish(self) -> Result<ProjectReport, ProjectInputError> {
        self.finish_with_timings().map(|(report, _, _)| report)
    }

    pub fn finish_with_timings(
        self,
    ) -> Result<(ProjectReport, std::time::Duration, std::time::Duration), ProjectInputError> {
        let input = ProjectInput {
            root: self.root,
            sources: self.sources.into_values().collect(),
            resolutions: self.resolutions.into_iter().collect(),
        }
        .validate()?;
        self.linter
            .lint_analyzed_project_timed(input, self.analyzed, self.parse_diagnostics)
    }
}

fn normalize_root(path: &Path) -> Result<PathBuf, ProjectInputError> {
    if path.as_os_str().is_empty() {
        Err(ProjectInputError::InvalidPath(String::new()))
    } else {
        Ok(path.to_path_buf())
    }
}
fn normalize_relative(path: &str) -> Result<String, ProjectInputError> {
    let original = path.to_string();
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.split('/').any(|part| part == "..")
    {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        Err(ProjectInputError::InvalidPath(original))
    } else {
        Ok(parts.join("/"))
    }
}
fn normalize_outside_target(path: &str) -> Result<String, ProjectInputError> {
    let original = path.to_string();
    let path = path.replace('\\', "/");
    if path.is_empty() || path.contains('\0') {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if absolute {
                continue;
            }
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
        } else {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err(ProjectInputError::InvalidPath(original));
    }
    Ok(if absolute {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    })
}
fn normalize_result(result: &mut ResolutionResult) -> Result<(), ProjectInputError> {
    match result {
        ResolutionResult::Internal { path } => *path = normalize_relative(path)?,
        ResolutionResult::OutsideProject { path } => *path = normalize_outside_target(path)?,
        ResolutionResult::External { package } if package.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(package.clone()));
        }
        ResolutionResult::Builtin { name } if name.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(name.clone()));
        }
        ResolutionResult::Unsupported { reason } if reason.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(reason.clone()));
        }
        _ => {}
    }
    Ok(())
}

impl SourceFile {
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            language: SourceLanguage::from_filename(&path),
            path,
            source: source.into(),
        }
    }
}

impl ProjectFinding {
    pub(crate) fn from_finding(finding: crate::Finding, path: &str) -> Self {
        let mut finding = Self {
            rule_id: finding.rule_id,
            message_id: finding.message_id,
            message: finding.message,
            severity: finding.severity,
            location: SourceLocation {
                path: path.to_owned(),
                range: finding.range,
            },
            evidence: finding
                .evidence
                .into_iter()
                .map(|evidence| ProjectEvidence {
                    message: evidence.message,
                    location: evidence.range.map(|range| SourceLocation {
                        path: path.to_owned(),
                        range,
                    }),
                    source: evidence.source,
                })
                .collect(),
        };
        dedup_evidence_in_order(&mut finding.evidence);
        finding
    }

    pub(crate) fn append_related(&mut self, evidence: impl IntoIterator<Item = ProjectEvidence>) {
        self.evidence.extend(evidence);
        dedup_evidence_in_order(&mut self.evidence);
    }
}

fn dedup_evidence_in_order(evidence: &mut Vec<ProjectEvidence>) {
    let mut seen = Vec::new();
    evidence.retain(|item| {
        let key = (
            item.message.clone(),
            item.location.clone(),
            item.source.clone(),
        );
        if seen.iter().any(|existing| existing == &key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rule::{
        ApiRule, ApiSeverity, CallMatcher, Confidence, FlowMatcher, FlowValueMatcher, Matcher,
    };
    use crate::{Position, SourceRange};

    fn test_linter() -> crate::Linter {
        let rule = ApiRule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
        )
    }

    fn flow_linter() -> crate::Linter {
        let rule = ApiRule::builder("flow.append")
            .label("Appends a configured script")
            .category("flow")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(
                FlowMatcher::new("script insertion")
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .property_write("src", FlowValueMatcher::Any)
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment
            .add_globals(["document", "url"])
            .expect("test environment globals");
        crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
        )
    }

    fn key(importer: &str) -> ResolutionRequestKey {
        ResolutionRequestKey {
            importer: importer.into(),
            kind: ResolutionRequestKind::Import,
            range: SourceRange {
                start: Position { line: 1, column: 1 },
                end: Position { line: 1, column: 8 },
            },
        }
    }

    #[test]
    fn validation_normalizes_and_sorts_sources_and_edges() {
        let input = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("./z.js", ""), SourceFile::new("a.js", "")],
            resolutions: vec![(
                key("./z.js"),
                ResolutionResult::Internal {
                    path: "./a.js".into(),
                },
            )],
        }
        .validate()
        .unwrap();

        assert_eq!(
            input
                .sources
                .iter()
                .map(|source| source.path.as_str())
                .collect::<Vec<_>>(),
            ["a.js", "z.js"]
        );
        assert_eq!(input.resolutions[0].0.importer, "z.js");
        assert_eq!(
            input.resolutions[0].1,
            ResolutionResult::Internal {
                path: "a.js".into()
            }
        );
        assert_eq!(input.module_ids()["a.js"], ModuleId(0));
        assert_eq!(input.module_ids()["z.js"], ModuleId(1));
    }

    #[test]
    fn duplicate_and_foreign_records_are_rejected() {
        let duplicate = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", ""), SourceFile::new("./a.js", "")],
            resolutions: vec![],
        }
        .validate();
        assert!(matches!(
            duplicate,
            Err(ProjectInputError::DuplicateSource(_))
        ));

        let foreign = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", "")],
            resolutions: vec![(key("missing.js"), ResolutionResult::Missing)],
        }
        .validate();
        assert!(matches!(
            foreign,
            Err(ProjectInputError::UnknownImporter(_))
        ));

        let malformed_range = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", "")],
            resolutions: vec![(
                ResolutionRequestKey {
                    importer: "a.js".into(),
                    kind: ResolutionRequestKind::Import,
                    range: SourceRange {
                        start: Position { line: 1, column: 1 },
                        end: Position { line: 1, column: 0 },
                    },
                },
                ResolutionResult::Missing,
            )],
        }
        .validate();
        assert!(matches!(
            malformed_range,
            Err(ProjectInputError::InvalidRange(_))
        ));
    }

    #[test]
    fn session_uses_project_analysis_and_preserves_single_file_findings() {
        let linter = test_linter();
        let source = "fetch('/remote');\n";
        let direct = linter.lint(source, "a.js");

        let mut session = linter.begin_project("/project").unwrap();
        session.add_source(SourceFile::new("a.js", source)).unwrap();
        let project = session.finish().unwrap();

        assert_eq!(project.files.len(), 1);
        assert_eq!(project.files[0].path, "a.js");
        assert_eq!(project.files[0].findings.len(), direct.findings.len());
        assert_eq!(
            project.files[0].findings[0].location.range,
            direct.findings[0].range
        );
        assert_eq!(project.files[0].findings[0].location.path, "a.js");
        assert_eq!(
            project.files[0].findings[0].evidence[0]
                .location
                .as_ref()
                .map(|location| location.path.as_str()),
            Some("a.js")
        );
    }

    #[test]
    fn linked_internal_aliases_preserve_external_and_global_call_identity() {
        let external_rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let global_rule = ApiRule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment(
                "test",
                vec![external_rule, global_rule],
                environment,
            )
            .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
        assert_eq!(
            main_report.findings[0].rule_id.as_str(),
            "test:network.request"
        );

        let mut global = linter.begin_project("/project").unwrap();
        let helper = global
            .add_source(SourceFile::new("helper.js", "export { fetch as send };"))
            .unwrap();
        let main = global
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        assert!(helper.is_empty());
        global
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = global.finish().unwrap();
        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
        assert_eq!(
            main_report.findings[0].rule_id.as_str(),
            "test:network.fetch"
        );
    }

    #[test]
    fn project_flow_crosses_an_exported_helper_boundary() {
        let linter = flow_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "helper.js",
                "export function append(element) { element.src = url; document.head.appendChild(element); }",
            ))
            .unwrap();
        let request = session
            .add_source(SourceFile::new(
                "main.js",
                "import { append } from './helper'; const element = document.createElement('script'); append(element);",
            ))
            .unwrap()
            .into_iter()
            .next()
            .expect("the helper import is a resolution request");
        session
            .record_resolution(
                request.key,
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        let main = report
            .files
            .iter()
            .find(|file| file.path == "helper.js")
            .expect("helper report");
        assert_eq!(main.findings.len(), 1);
        assert_eq!(main.findings[0].location.path, "helper.js");
    }

    #[test]
    fn project_flow_preserves_requirements_through_a_helper_chain() {
        let linter = flow_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "sink.js",
                "export function finish(element) { document.head.appendChild(element); }",
            ))
            .unwrap();
        let helper_request = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { finish } from './sink'; export function append(element) { element.src = url; finish(element); }",
            ))
            .unwrap();
        let main_request = session
            .add_source(SourceFile::new(
                "main.js",
                "import { append } from './helper'; const element = document.createElement('script'); append(element);",
            ))
            .unwrap();
        session
            .record_resolution(
                helper_request[0].key.clone(),
                ResolutionResult::Internal {
                    path: "sink.js".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main_request[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert!(report.files.iter().any(|file| !file.findings.is_empty()));
    }

    #[test]
    fn project_flow_follows_a_returned_parameter() {
        let linter = flow_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "helper.js",
                "export function identity(element) { return element; }",
            ))
            .unwrap();
        let request = session
            .add_source(SourceFile::new(
                "main.js",
                "import { identity } from './helper'; const element = document.createElement('script'); const returned = identity(element); returned.src = url; document.head.appendChild(returned);",
            ))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        session
            .record_resolution(
                request.key,
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        let main = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main.findings.len(), 1);
    }

    #[test]
    fn project_flow_fails_closed_for_unsupported_helper_control_flow() {
        let linter = flow_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "helper.js",
                "export function append(element) { if (ready) element.src = url; document.head.appendChild(element); }",
            ))
            .unwrap();
        let request = session
            .add_source(SourceFile::new(
                "main.js",
                "import { append } from './helper'; const element = document.createElement('script'); append(element);",
            ))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        session
            .record_resolution(
                request.key,
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert!(report.files.iter().all(|file| file.findings.is_empty()));
    }

    #[test]
    fn linked_unknown_exports_and_importer_reassignment_fail_closed() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send = local; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            0
        );

        let mut missing = linter.begin_project("/project").unwrap();
        let main = missing
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        missing
            .add_source(SourceFile::new("helper.js", "export const other = 1;"))
            .unwrap();
        missing
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = missing.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            0
        );
    }

    #[test]
    fn unresolved_internal_imports_do_not_become_external_provenance() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("./helper", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "main.js",
                "import { request } from './helper'; request();",
            ))
            .unwrap();
        let report = session.finish().unwrap();

        assert!(report.files.iter().all(|file| file.findings.is_empty()));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unresolved_internal_request")
        );
    }

    #[test]
    fn commonjs_export_aliases_preserve_external_provenance_across_modules() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "const { request } = require('web'); exports.send = request;",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "const { send } = require('./helper'); send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();

        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
    }

    #[test]
    fn namespace_imports_follow_star_reexports() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request };",
            ))
            .unwrap();
        let barrel = session
            .add_source(SourceFile::new("barrel.js", "export * from './helper';"))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import * as api from './barrel'; api.request();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                barrel[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "barrel.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();

        let main_report = report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap();
        assert_eq!(main_report.findings.len(), 1);
    }

    #[test]
    fn static_dynamic_imports_follow_namespace_exports() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "async function run() { const api = await import('./helper'); api.request(); }",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn anonymous_commonjs_functions_remain_callable_across_modules() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "const { request } = require('web'); exports.send = () => request();",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "const { send } = require('./helper'); send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "helper.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn returned_callable_provenance_crosses_an_exported_function() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export function get() { return request; }",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { get } from './helper'; get()('/remote');",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn linked_external_call_arguments_are_projected_after_reexports() {
        let rule = ApiRule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send('/remote');",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();

        assert_eq!(
            report
                .files
                .iter()
                .find(|file| file.path == "main.js")
                .unwrap()
                .findings
                .len(),
            1
        );
    }

    #[test]
    fn project_keeps_sorted_parse_failures_separate_from_valid_modules() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new("z.js", "function {"))
            .unwrap();
        session
            .add_source(SourceFile::new("a.js", "fetch('/remote');"))
            .unwrap();

        let report = session.finish().unwrap();
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["a.js", "z.js"]
        );
        assert_eq!(report.files[0].findings.len(), 1);
        assert_eq!(report.files[1].findings.len(), 0);
        assert_eq!(report.files[1].parse_diagnostics.len(), 1);
    }

    #[test]
    fn session_returns_static_import_dynamic_import_require_and_reexport_requests() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        let requests = session
            .add_source(SourceFile::new(
                "main.js",
                "import { value as local } from './dep';\nexport { local as renamed } from './dep';\nconst x = require('./cjs');\nimport('./lazy');",
            ))
            .unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests
                .iter()
                .map(|request| request.key.kind)
                .collect::<Vec<_>>(),
            vec![
                ResolutionRequestKind::Import,
                ResolutionRequestKind::Import,
                ResolutionRequestKind::Require,
                ResolutionRequestKind::DynamicImport,
            ]
        );
        assert_eq!(requests[0].request, "./dep");
        assert_eq!(requests[2].request, "./cjs");
        assert_eq!(requests[3].request, "./lazy");
        assert_eq!(requests[2].key.range.start.column, 19);
        assert_eq!(requests[2].key.range.end.column, 26);
    }

    #[test]
    fn session_rejects_resolution_for_an_unauthored_request() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new("main.js", "fetch('/remote');"))
            .unwrap();
        let error = session.record_resolution(key("main.js"), ResolutionResult::Missing);
        assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
    }

    #[test]
    fn rejected_duplicate_source_does_not_replace_the_original() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new("main.js", "fetch('/remote');"))
            .unwrap();
        let error = session.add_source(SourceFile::new("./main.js", ""));
        assert!(matches!(error, Err(ProjectInputError::DuplicateSource(_))));

        let report = session.finish().unwrap();
        assert_eq!(report.files[0].findings.len(), 1);
    }

    #[test]
    fn type_only_reexports_do_not_create_runtime_requests() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        let requests = session
            .add_source(SourceFile::new(
                "types.ts",
                "export { type Foo } from './dependency';",
            ))
            .unwrap();
        assert!(requests.is_empty());
    }

    #[test]
    fn linker_accepts_named_reexports_and_reports_missing_exports() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        let dep_requests = session
            .add_source(SourceFile::new("dep.js", "export const value = 1;"))
            .unwrap();
        assert!(dep_requests.is_empty());
        let barrel_requests = session
            .add_source(SourceFile::new(
                "barrel.js",
                "export { value } from './dep';",
            ))
            .unwrap();
        let main_requests = session
            .add_source(SourceFile::new(
                "main.js",
                "import { value } from './barrel';",
            ))
            .unwrap();
        session
            .record_resolution(
                barrel_requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "dep.js".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main_requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "barrel.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );

        let mut missing = linter.begin_project("/project").unwrap();
        let requests = missing
            .add_source(SourceFile::new("main.js", "import { nope } from './dep';"))
            .unwrap();
        missing
            .add_source(SourceFile::new("dep.js", "export const value = 1;"))
            .unwrap();
        missing
            .record_resolution(
                requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "dep.js".into(),
                },
            )
            .unwrap();
        let report = missing.finish().unwrap();
        assert_eq!(report.diagnostics[0].code, "missing_imported_export");
    }

    #[test]
    fn linker_reports_ambiguous_multiple_star_exports() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new("a.js", "export const value = 1;"))
            .unwrap();
        session
            .add_source(SourceFile::new("b.js", "export const value = 2;"))
            .unwrap();
        let barrel_requests = session
            .add_source(SourceFile::new(
                "barrel.js",
                "export * from './a'; export * from './b';",
            ))
            .unwrap();
        let main_requests = session
            .add_source(SourceFile::new(
                "main.js",
                "import { value } from './barrel';",
            ))
            .unwrap();
        session
            .record_resolution(
                barrel_requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "a.js".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                barrel_requests[1].key.clone(),
                ResolutionResult::Internal {
                    path: "b.js".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main_requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "barrel.js".into(),
                },
            )
            .unwrap();

        let report = session.finish().unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ambiguous_star_export")
        );
    }

    #[test]
    fn outside_project_targets_accept_normalized_absolute_paths() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        let requests = session
            .add_source(SourceFile::new("main.js", "import value from './outside';"))
            .unwrap();
        session
            .record_resolution(
                requests[0].key.clone(),
                ResolutionResult::OutsideProject {
                    path: "/other/./dependency.js".into(),
                },
            )
            .unwrap();
        let report = session.finish().unwrap();
        assert_eq!(report.diagnostics[0].code, "outside_project_target");
    }

    #[test]
    fn dynamic_commonjs_export_shapes_are_reported_and_fail_closed() {
        let linter = test_linter();
        let mut session = linter.begin_project("/project").unwrap();
        let main_requests = session
            .add_source(SourceFile::new(
                "main.js",
                "import { value } from './dependency';",
            ))
            .unwrap();
        session
            .add_source(SourceFile::new(
                "dependency.js",
                "module.exports = { value: 1, ...extra };",
            ))
            .unwrap();
        session
            .record_resolution(
                main_requests[0].key.clone(),
                ResolutionResult::Internal {
                    path: "dependency.js".into(),
                },
            )
            .unwrap();

        let report = session.finish().unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unsupported_commonjs_exports")
        );
    }
}
