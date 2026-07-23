//! Local analysis artifact management and cache helpers.
//!
//! Owns the artifact map, parse diagnostic map, authored-request table, and
//! cache-lookup helpers that phase-state types delegate to.

use std::{collections::BTreeMap, sync::Arc};

use super::execution::ExecutionEvent;
use crate::{
    ParseDiagnostic, ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, SourceFile,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, LoweredSource,
        SharedSemanticArtifact,
    },
};

#[derive(Default)]
pub(super) struct AnalysisArtifacts {
    pub(super) authored_requests: BTreeMap<ResolutionRequestKey, ResolutionRequest>,
    pub(super) analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
    pub(super) parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
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
        let local = LocalArtifact::new(lowered.source.clone(), lowered.semantic);
        let requests = local
            .interface()
            .authored_requests(path, &local.source_context().lines);
        for request in &requests {
            self.authored_requests
                .insert(request.key.clone(), request.clone());
        }
        self.analyzed.insert(path.clone(), local);
        requests
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
    LoweredSource {
        source: LocatedSourceContext::new(source),
        semantic: Arc::clone(&cached.semantic),
    }
}

pub(super) fn insert_and_notify(
    cache: &ArtifactCacheHandle,
    key: ArtifactCacheKey,
    lowered: &LoweredSource,
    observer: &dyn super::execution::ExecutionObserver,
) {
    let evicted = cache.insert(
        key,
        SharedSemanticArtifact {
            semantic: Arc::clone(&lowered.semantic),
        },
    );
    observer.observe(ExecutionEvent::CacheInserted);
    if evicted {
        observer.observe(ExecutionEvent::CacheEvicted);
    }
}
