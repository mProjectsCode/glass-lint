//! Local analysis artifact management and cache helpers.
//!
//! Owns per-source local-analysis outcomes, the authored-request table, and
//! the consuming transition to validated linker input, together with the
//! cache-lookup helpers that phase-state types delegate to.

use std::collections::BTreeMap;

use crate::{
    ParseDiagnostic,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LoweredSource, QualifiedRequestId,
        ResolvedLinkInput, model::module::ModuleRequestId,
    },
    project::{
        ModuleId, ProjectPhaseError, ProjectRelativePath, ResolutionRequest, ResolutionRequestKey,
        ResolutionTable, ResolverOutcome, SourceTable,
        session::{ExecutionEvent, ExecutionObserver},
    },
};

/// Pre-computed index of authored requests for membership validation and
/// qualified-ID construction. Built once during lowering and reused during
/// resolution, avoiding per-module re-traversal of the module interface.
#[derive(Default)]
pub struct AuthoredRequestTable {
    /// Key → ModuleRequestId for membership and qualified-ID production.
    by_key: BTreeMap<ResolutionRequestKey, ModuleRequestId>,
}

impl AuthoredRequestTable {
    pub fn contains_key(&self, key: &ResolutionRequestKey) -> bool {
        self.by_key.contains_key(key)
    }

    pub(super) fn insert(&mut self, key: ResolutionRequestKey, id: ModuleRequestId) {
        self.by_key.insert(key, id);
    }

    pub(super) fn qualified_ids(
        &self,
        module_ids: &BTreeMap<ProjectRelativePath, ModuleId>,
    ) -> Result<BTreeMap<ResolutionRequestKey, QualifiedRequestId>, ProjectPhaseError> {
        self.by_key
            .iter()
            .map(|(key, req_id)| {
                let module = module_ids.get(key.importer()).copied().ok_or_else(|| {
                    ProjectPhaseError::UnknownImporter(key.importer().as_str().to_owned())
                })?;
                Ok((key.clone(), QualifiedRequestId::new(module, *req_id)))
            })
            .collect()
    }
}

#[derive(Default)]
pub struct AnalysisArtifacts {
    authored_requests: AuthoredRequestTable,
    outcomes: BTreeMap<ProjectRelativePath, LocalAnalysisOutcome>,
}

enum LocalAnalysisOutcome {
    Analyzed(LocalArtifact),
    ParseFailed(ParseDiagnostic),
}

/// Authored module requests produced by completed local source analysis.
/// Source and artifact storage remains owned by the collection phase.
pub struct AuthoredRequests(Vec<ResolutionRequest>);

impl AuthoredRequests {
    pub(super) fn new(requests: Vec<ResolutionRequest>) -> Self {
        Self(requests)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ResolutionRequest> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl IntoIterator for AuthoredRequests {
    type IntoIter = std::vec::IntoIter<ResolutionRequest>;
    type Item = ResolutionRequest;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a AuthoredRequests {
    type IntoIter = std::slice::Iter<'a, ResolutionRequest>;
    type Item = &'a ResolutionRequest;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl AnalysisArtifacts {
    pub(super) fn validate_complete(&self, sources: &SourceTable) -> Result<(), ProjectPhaseError> {
        let incomplete = sources
            .in_path_order()
            .filter(|(path, _)| self.needs_analysis(path))
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        if incomplete.is_empty() {
            Ok(())
        } else {
            Err(ProjectPhaseError::IncompleteLocalAnalysis(incomplete))
        }
    }

    pub(super) fn record_parse_failure(
        &mut self,
        path: ProjectRelativePath,
        error: ParseDiagnostic,
    ) {
        self.outcomes
            .insert(path, LocalAnalysisOutcome::ParseFailed(error));
    }

    pub(super) fn record_lowered(
        &mut self,
        path: &ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        let local = LocalArtifact::from_lowered(lowered);
        let with_ids = local
            .interface()
            .requests_with_ids(path, local.source_context().lines());
        for (req_id, request) in &with_ids {
            self.authored_requests
                .insert(request.key().clone(), *req_id);
        }
        self.outcomes
            .insert(path.clone(), LocalAnalysisOutcome::Analyzed(local));
        with_ids.into_iter().map(|(_, request)| request).collect()
    }

    /// Whether a source path still needs local analysis: it has neither a
    /// completed artifact nor a recorded parse failure.
    pub(super) fn needs_analysis(&self, path: &ProjectRelativePath) -> bool {
        !self.outcomes.contains_key(path)
    }

    /// Whether a resolution request key was authored by a completed local
    /// analysis, which is the prerequisite for a valid resolver outcome.
    pub(super) fn is_authored_request(&self, key: &ResolutionRequestKey) -> bool {
        self.authored_requests.contains_key(key)
    }

    /// One consuming transition from locally analyzed artifacts to validated
    /// linker input. Resolver outcomes are checked against the authored
    /// request table, stable module and request identities are assigned from
    /// source order, and parse diagnostics are split off for report assembly.
    pub(super) fn into_link_input(
        self,
        sources: &SourceTable,
        outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
    ) -> Result<
        (
            ResolvedLinkInput,
            BTreeMap<ProjectRelativePath, ParseDiagnostic>,
        ),
        ProjectPhaseError,
    > {
        let mut resolutions = ResolutionTable::default();
        for (key, result) in outcomes {
            if !self.is_authored_request(&key) {
                return Err(ProjectPhaseError::UnknownRequest(key));
            }
            let result = result.validate()?;
            resolutions.insert(key, result)?;
        }
        let Self {
            authored_requests,
            outcomes,
        } = self;
        let mut analyzed = BTreeMap::new();
        let mut parse_diagnostics = BTreeMap::new();
        for (path, outcome) in outcomes {
            match outcome {
                LocalAnalysisOutcome::Analyzed(local) => {
                    analyzed.insert(path, local);
                }
                LocalAnalysisOutcome::ParseFailed(diagnostic) => {
                    parse_diagnostics.insert(path, diagnostic);
                }
            }
        }
        let module_ids = sources.module_ids()?;
        let request_ids = authored_requests.qualified_ids(&module_ids)?;
        let link_input =
            ResolvedLinkInput::build(analyzed, &module_ids, resolutions, &request_ids)?;
        Ok((link_input, parse_diagnostics))
    }
}

pub(super) fn insert_and_notify(
    cache: &ArtifactCacheHandle,
    key: ArtifactCacheKey,
    lowered: &LoweredSource,
    observer: &dyn ExecutionObserver,
) {
    let evicted = cache.insert_lowered(key, lowered);
    observer.observe(ExecutionEvent::CacheInserted);
    if evicted {
        observer.observe(ExecutionEvent::CacheEvicted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnalysisLimits, Environment,
        analysis::Lowerer,
        project::{DiagnosticCode, ResolutionRequestKind, SourceFile},
    };

    fn lower(path: &str, source: &str) -> (ProjectRelativePath, LoweredSource) {
        let source = SourceFile::new(path, source).unwrap();
        let lowered = Lowerer::new(&Environment::default(), &AnalysisLimits::default())
            .lower_source(&source)
            .unwrap();
        (source.path().clone(), lowered)
    }

    fn parse_failure(path: &str) -> ParseDiagnostic {
        ParseDiagnostic {
            code: DiagnosticCode::new("syntax_error").unwrap(),
            message: "invalid syntax".into(),
            filename: path.into(),
            range: None,
            failure: crate::parse::ParseFailureKind::Syntax,
        }
    }

    #[test]
    fn needs_analysis_tracks_completed_and_failed_sources() {
        let mut artifacts = AnalysisArtifacts::default();
        let (analyzed_path, lowered) = lower("a.js", "fetch('/x');");
        assert!(artifacts.needs_analysis(&analyzed_path));
        artifacts.record_lowered(&analyzed_path, lowered);
        assert!(!artifacts.needs_analysis(&analyzed_path));

        let failed_path = ProjectRelativePath::new("b.js").unwrap();
        assert!(artifacts.needs_analysis(&failed_path));
        artifacts.record_parse_failure(failed_path.clone(), parse_failure("b.js"));
        assert!(!artifacts.needs_analysis(&failed_path));
    }

    #[test]
    fn successful_retry_replaces_a_parse_failure() {
        let source = SourceFile::new("retry.js", "fetch('/x');").unwrap();
        let mut sources = SourceTable::default();
        sources.insert(source.clone()).unwrap();
        let mut artifacts = AnalysisArtifacts::default();
        artifacts.record_parse_failure(source.path().clone(), parse_failure("retry.js"));
        artifacts.record_lowered(
            source.path(),
            lower(source.path().as_str(), "fetch('/x');").1,
        );

        let (_, diagnostics) = artifacts.into_link_input(&sources, []).unwrap();
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_failure_replaces_a_previous_success() {
        let source = SourceFile::new("retry.js", "fetch('/x');").unwrap();
        let mut sources = SourceTable::default();
        sources.insert(source.clone()).unwrap();
        let mut artifacts = AnalysisArtifacts::default();
        artifacts.record_lowered(
            source.path(),
            lower(source.path().as_str(), "fetch('/x');").1,
        );
        artifacts.record_parse_failure(source.path().clone(), parse_failure("retry.js"));

        let (_, diagnostics) = artifacts.into_link_input(&sources, []).unwrap();
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn qualified_ids_reject_missing_importer_modules() {
        let source = SourceFile::new("missing.js", "import value from './dep.js';").unwrap();
        let mut artifacts = AnalysisArtifacts::default();
        artifacts.record_lowered(
            source.path(),
            Lowerer::new(&Environment::default(), &AnalysisLimits::default())
                .lower_source(&source)
                .unwrap(),
        );

        assert_eq!(
            artifacts.authored_requests.qualified_ids(&BTreeMap::new()),
            Err(ProjectPhaseError::UnknownImporter("missing.js".into()))
        );
    }

    #[test]
    fn into_link_input_accepts_authored_and_rejects_unknown_outcomes() {
        let source = SourceFile::new("main.js", "import value from './dep.js';").unwrap();
        let mut sources = SourceTable::default();
        sources.insert(source.clone()).unwrap();

        let (link_input, parse_diagnostics) = {
            let mut artifacts = AnalysisArtifacts::default();
            let requests = artifacts.record_lowered(
                source.path(),
                Lowerer::new(&Environment::default(), &AnalysisLimits::default())
                    .lower_source(&source)
                    .unwrap(),
            );
            let key = requests[0].key().clone();
            artifacts
                .into_link_input(&sources, [(key, ResolverOutcome::Missing)])
                .unwrap()
        };
        assert!(parse_diagnostics.is_empty());
        assert_eq!(link_input.resolution_count(), 1);

        let mut artifacts = AnalysisArtifacts::default();
        let requests = artifacts.record_lowered(
            source.path(),
            Lowerer::new(&Environment::default(), &AnalysisLimits::default())
                .lower_source(&source)
                .unwrap(),
        );
        let mut unknown = requests[0].key().clone();
        unknown = ResolutionRequestKey::new(
            unknown.importer().clone(),
            ResolutionRequestKind::Require,
            unknown.range(),
        );
        let error = artifacts.into_link_input(&sources, [(unknown, ResolverOutcome::Missing)]);
        assert!(matches!(error, Err(ProjectPhaseError::UnknownRequest(_))));
    }
}
