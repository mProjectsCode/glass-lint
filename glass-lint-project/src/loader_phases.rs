use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glass_lint_core::project::{
    ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind,
    ResolverOutcome,
};

use crate::{boundary::AcceptedSourcePath, error::ProjectLoadError, resolver::ProjectResolver};

#[derive(Default)]
pub struct PathWorkQueue {
    queue: VecDeque<AcceptedSourcePath>,
    seen: BTreeSet<AcceptedSourcePath>,
}

impl PathWorkQueue {
    pub(super) fn extend(&mut self, paths: impl IntoIterator<Item = AcceptedSourcePath>) {
        for path in paths {
            self.push(path);
        }
    }

    pub(super) fn pop_front(&mut self) -> Option<AcceptedSourcePath> {
        self.queue.pop_front()
    }

    pub(super) fn push(&mut self, path: AcceptedSourcePath) {
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
            importer: request.importer().clone(),
            kind: request.kind(),
            specifier: request.specifier().to_string(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ResolutionCache {
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
        let cache_key = request.key().clone();
        match self.by_key.entry(cache_key) {
            std::collections::btree_map::Entry::Occupied(entry) => Ok((entry.into_mut(), false)),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let specifier_key = ResolutionSpecifierKey::from_request(request);
                let (outcome, did_resolve) = match self.by_specifier.entry(specifier_key) {
                    std::collections::btree_map::Entry::Occupied(entry) => {
                        (entry.get().clone(), false)
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        let outcome = resolver.resolve(request)?;
                        entry.insert(outcome.clone());
                        (outcome, true)
                    }
                };
                let cached = entry.insert(outcome);
                Ok((cached, did_resolve))
            }
        }
    }

    pub(super) fn into_iter(self) -> impl Iterator<Item = (ResolutionRequestKey, ResolverOutcome)> {
        self.by_key.into_iter()
    }
}
