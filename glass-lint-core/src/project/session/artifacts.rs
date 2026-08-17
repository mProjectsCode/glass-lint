//! Local analysis artifact management and cache helpers.
//!
//! Owns per-source local-analysis outcomes, the authored-request table, and
//! the consuming transition to validated linker input, together with the
//! cache-lookup helpers that phase-state types delegate to.

use std::collections::BTreeMap;

use crate::{
    ParseDiagnostic,
    analysis::{
        AnalyzedSource, ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, QualifiedRequestId,
        ResolvedLinkInput,
        model::module::{ModuleRequestId, ModuleRequestRole},
    },
    project::{
        ModuleId, ProjectPhaseError, ProjectRelativePath, ResolutionRequest, ResolutionRequestKey,
        ResolutionRequestKind, ResolutionTable, ResolverOutcome, SourceTable,
        session::{ExecutionEvent, ExecutionObserver},
    },
};

/// Pre-computed index of authored requests for membership validation and
/// qualified-ID construction. Built once during local analysis and reused
/// during resolution, avoiding per-module re-traversal of the module interface.
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
            .in_normalized_path_order()
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

    pub(super) fn record_analyzed(
        &mut self,
        path: &ProjectRelativePath,
        analyzed: AnalyzedSource,
    ) -> Vec<ResolutionRequest> {
        self.record_local(path, LocalArtifact::from_analyzed(analyzed))
    }

    pub(super) fn record_local(
        &mut self,
        path: &ProjectRelativePath,
        local: LocalArtifact,
    ) -> Vec<ResolutionRequest> {
        let mut authored_requests = Vec::new();
        for (req_id, request) in local.interface().request_entries() {
            let Some(range) = local
                .source_context()
                .lines()
                .try_range(request.span())
                .ok()
            else {
                continue;
            };
            let kind = match request.role() {
                ModuleRequestRole::Import { .. }
                | ModuleRequestRole::ReExport
                | ModuleRequestRole::StarExport => ResolutionRequestKind::StaticImport,
                ModuleRequestRole::DynamicImport => ResolutionRequestKind::DynamicImport,
                ModuleRequestRole::Require => ResolutionRequestKind::Require,
            };
            let request = ResolutionRequest::new(
                ResolutionRequestKey::new(path.clone(), kind, range),
                request.specifier().clone(),
            );
            self.authored_requests.insert(request.key().clone(), req_id);
            authored_requests.push(request);
        }
        self.outcomes
            .insert(path.clone(), LocalAnalysisOutcome::Analyzed(local));
        authored_requests
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
    analyzed: &AnalyzedSource,
    observer: &dyn ExecutionObserver,
) {
    let evicted = cache.insert_analyzed(key, analyzed);
    observer.observe(ExecutionEvent::CacheInserted);
    if evicted {
        observer.observe(ExecutionEvent::CacheEvicted);
    }
}

#[cfg(test)]
mod tests;
