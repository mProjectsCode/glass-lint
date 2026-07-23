//! Local analysis artifact management and cache helpers.
//!
//! Owns the artifact map, parse diagnostic map, authored-request table, and
//! cache-lookup helpers that phase-state types delegate to.

use std::{collections::BTreeMap, sync::Arc};

use super::execution::ExecutionEvent;
use crate::{
    ParseDiagnostic,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, LoweredSource,
        SharedSemanticArtifact, module::ModuleRequestId,
    },
    project::{ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, SourceFile},
};

/// Pre-computed index of authored requests for membership validation and
/// qualified-ID construction. Built once during lowering and reused during
/// resolution, avoiding per-module re-traversal of the module interface.
#[derive(Default)]
pub(super) struct AuthoredRequestTable {
    /// Key → ModuleRequestId for membership and qualified-ID production.
    by_key: BTreeMap<ResolutionRequestKey, ModuleRequestId>,
}

impl AuthoredRequestTable {
    pub(super) fn contains_key(&self, key: &ResolutionRequestKey) -> bool {
        self.by_key.contains_key(key)
    }

    pub(super) fn insert(&mut self, key: ResolutionRequestKey, id: ModuleRequestId) {
        self.by_key.insert(key, id);
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ResolutionRequestKey, ModuleRequestId)> {
        self.by_key.iter().map(|(key, id)| (key, *id))
    }
}

#[derive(Default)]
pub(super) struct AnalysisArtifacts {
    pub(super) authored_requests: AuthoredRequestTable,
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
        let with_ids = local
            .interface()
            .requests_with_ids(path, &local.source_context().lines);
        for (req_id, request) in &with_ids {
            self.authored_requests.insert(request.key.clone(), *req_id);
        }
        self.analyzed.insert(path.clone(), local);
        with_ids.into_iter().map(|(_, request)| request).collect()
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
