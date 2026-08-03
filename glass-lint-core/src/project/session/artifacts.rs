//! Local analysis artifact management and cache helpers.
//!
//! Owns the artifact map, parse diagnostic map, authored-request table, and
//! the consuming transition to validated linker input, together with the
//! cache-lookup helpers that phase-state types delegate to.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    ParseDiagnostic,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, LoweredSource,
        QualifiedRequestId, ResolvedLinkInput, SharedSemanticArtifact, module::ModuleRequestId,
    },
    project::{
        ModuleId, ProjectInputError, ProjectRelativePath, ResolutionRequest, ResolutionRequestKey,
        ResolutionTable, ResolverOutcome, SourceFile, SourceTable,
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
    ) -> BTreeMap<ResolutionRequestKey, QualifiedRequestId> {
        self.by_key
            .iter()
            .filter_map(|(key, req_id)| {
                let module = module_ids.get(key.importer()).copied()?;
                Some((key.clone(), QualifiedRequestId::new(module, *req_id)))
            })
            .collect()
    }
}

#[derive(Default)]
pub struct AnalysisArtifacts {
    authored_requests: AuthoredRequestTable,
    analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
    parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
}

/// Authored module requests produced by one completed local source analysis.
/// Source and artifact storage remains owned by the collection phase.
pub struct SourceAnalysis {
    pub(super) requests: Vec<ResolutionRequest>,
}

impl SourceAnalysis {
    pub fn requests(self) -> Vec<ResolutionRequest> {
        self.requests
    }

    pub fn requests_ref(&self) -> &[ResolutionRequest] {
        &self.requests
    }
}

impl AnalysisArtifacts {
    pub(super) fn record_parse_failure(
        &mut self,
        path: ProjectRelativePath,
        error: ParseDiagnostic,
    ) {
        self.analyzed.remove(&path);
        self.parse_diagnostics.insert(path, error);
    }

    pub(super) fn record_lowered(
        &mut self,
        path: &ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        let (source, semantic) = lowered.into_parts();
        let local = LocalArtifact::new(source, semantic);
        let with_ids = local
            .interface()
            .requests_with_ids(path, local.source_context().lines());
        for (req_id, request) in &with_ids {
            self.authored_requests
                .insert(request.key().clone(), *req_id);
        }
        self.analyzed.insert(path.clone(), local);
        with_ids.into_iter().map(|(_, request)| request).collect()
    }

    /// Whether a source path still needs local analysis: it has neither a
    /// completed artifact nor a recorded parse failure.
    pub(super) fn needs_analysis(&self, path: &ProjectRelativePath) -> bool {
        !self.analyzed.contains_key(path) && !self.parse_diagnostics.contains_key(path)
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
        ProjectInputError,
    > {
        let mut resolutions = ResolutionTable::default();
        for (key, result) in outcomes {
            let key = key.normalize()?;
            if !self.is_authored_request(&key) {
                return Err(ProjectInputError::UnknownRequest(key));
            }
            let result = result.normalize()?;
            resolutions.insert(key, result)?;
        }
        let Self {
            authored_requests,
            analyzed,
            parse_diagnostics,
        } = self;
        let module_ids = sources.module_ids()?;
        let request_ids = authored_requests.qualified_ids(&module_ids);
        let link_input =
            ResolvedLinkInput::build(analyzed, &module_ids, resolutions, &request_ids)?;
        Ok((link_input, parse_diagnostics))
    }
}

/// Outcome of looking up a source in the artifact cache.
pub(super) enum CacheLookup {
    Hit(LoweredSource),
    Miss(ArtifactCacheKey),
}

pub(super) fn cached_lowered_source(
    source: &SourceFile,
    cached: &SharedSemanticArtifact,
) -> LoweredSource {
    LoweredSource::new(
        LocatedSourceContext::with_index(source.path().clone(), Arc::clone(cached.source_index())),
        Arc::clone(cached.semantic()),
    )
}

pub(super) fn insert_and_notify(
    cache: &ArtifactCacheHandle,
    key: ArtifactCacheKey,
    lowered: &LoweredSource,
    observer: &dyn ExecutionObserver,
) {
    let evicted = cache.insert(key, SharedSemanticArtifact::from_lowered(lowered));
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
        assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
    }
}
