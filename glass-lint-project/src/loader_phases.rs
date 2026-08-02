use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glass_lint_core::project::{
    ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind,
    ResolverOutcome,
};

use crate::{
    admission::AdmittedSourcePath, error::ProjectLoadError, loader::ProjectMetricsAccumulator,
    resolver::ProjectResolver,
};

#[derive(Default)]
pub(super) struct PathWorkQueue {
    queue: VecDeque<AdmittedSourcePath>,
    seen: BTreeSet<AdmittedSourcePath>,
}

impl PathWorkQueue {
    pub(super) fn extend(&mut self, paths: impl IntoIterator<Item = AdmittedSourcePath>) {
        for path in paths {
            self.push(path);
        }
    }

    pub(super) fn pop_front(&mut self) -> Option<AdmittedSourcePath> {
        self.queue.pop_front()
    }

    pub(super) fn push(&mut self, path: AdmittedSourcePath) {
        if self.seen.insert(path.clone()) {
            self.queue.push_back(path);
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ResolutionSpecifierKey {
    importer: ProjectRelativePath,
    kind: ResolutionRequestKind,
    specifier: String,
}

impl ResolutionSpecifierKey {
    fn from_request(request: &ResolutionRequest) -> Self {
        Self {
            importer: request.key.importer.clone(),
            kind: request.key.kind,
            specifier: request.request.to_string(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ResolutionCache {
    /// Occurrence-keyed cache required by core (includes range).
    by_key: BTreeMap<ResolutionRequestKey, ResolverOutcome>,
    /// Semantic cache keyed by importer, request kind, and normalized
    /// specifier — catches repeated imports at different ranges.
    by_specifier: BTreeMap<ResolutionSpecifierKey, ResolverOutcome>,
}

impl ResolutionCache {
    pub(super) fn resolve_or_get(
        &mut self,
        request: &ResolutionRequest,
        resolver: &ProjectResolver,
    ) -> Result<(&ResolverOutcome, bool), ProjectLoadError> {
        let cache_key = request.key.clone();
        if self.by_key.contains_key(&cache_key) {
            let Some(outcome) = self.by_key.get(&cache_key) else {
                debug_assert!(false, "cache key disappeared after contains_key");
                return Err(ProjectLoadError::CacheInvariant);
            };
            return Ok((outcome, false));
        }

        let specifier_key = ResolutionSpecifierKey::from_request(request);
        let (outcome, did_resolve) = match self.by_specifier.entry(specifier_key) {
            std::collections::btree_map::Entry::Occupied(entry) => (entry.get().clone(), false),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let outcome = resolver.resolve(request)?;
                entry.insert(outcome.clone());
                (outcome, true)
            }
        };
        let cached = self.by_key.entry(cache_key).or_insert(outcome);
        Ok((cached, did_resolve))
    }

    pub(super) fn into_iter(self) -> impl Iterator<Item = (ResolutionRequestKey, ResolverOutcome)> {
        self.by_key.into_iter()
    }
}

#[derive(Debug, Default)]
pub(super) struct LoadProgress {
    requests: usize,
    edges: usize,
    source_bytes: u64,
}

impl LoadProgress {
    pub(super) fn source_bytes(&self) -> u64 {
        self.source_bytes
    }

    pub(super) fn add_requests(
        &mut self,
        count: usize,
        limit: usize,
    ) -> Result<(), ProjectLoadError> {
        self.requests = self
            .requests
            .checked_add(count)
            .ok_or(ProjectLoadError::TooManyRequests(limit))?;
        if self.requests > limit {
            return Err(ProjectLoadError::TooManyRequests(limit));
        }
        Ok(())
    }

    pub(super) fn record_edge(&mut self) {
        self.edges = self.edges.saturating_add(1);
    }

    pub(super) fn record_source_bytes(
        &mut self,
        bytes: u64,
        limit: u64,
    ) -> Result<(), ProjectLoadError> {
        self.source_bytes = self.source_bytes.saturating_add(bytes);
        if self.source_bytes > limit {
            return Err(ProjectLoadError::ProjectSourceTooLarge {
                bytes: self.source_bytes,
                limit,
            });
        }
        Ok(())
    }

    pub(super) fn publish(&self, metrics: &mut ProjectMetricsAccumulator) {
        metrics.requests = self.requests;
        metrics.edges = self.edges;
        metrics.bytes = self.source_bytes;
    }
}
