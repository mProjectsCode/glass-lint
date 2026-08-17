//! Deterministic project input, local analysis, and staging.
//!
//! Session state and project analysis live here. The execution runtime and
//! artifact-management helpers are in sibling submodules.

mod artifacts;
pub(super) mod execution;

use std::num::NonZeroUsize;

pub use artifacts::{AnalysisArtifacts, AuthoredRequests};
#[cfg(test)]
pub(super) use execution::{
    ControlledLocalJobExecutor, ControlledReleaseOrder, CountingExecutionObserver,
    InvocationCounts, outstanding_job_bound,
};
use execution::{
    ExecutionEvent, ExecutionObserver, LocalJob, LocalJobCallbacks, LocalJobCandidate,
    LocalJobExecutor, LocalJobResult, NoopExecutionObserver, ThreadLocalJobExecutor,
    analyze_with_observer, normalize_worker_limit,
};

use crate::{
    AnalysisLimits, Environment, ParseDiagnostic, ProjectAdmissionLimits, RuleCatalog,
    analysis::{ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, SemanticAnalyzer},
    api::classification::RuleIndex,
    lint::{ProjectAnalysis, report::ProjectReportAssembler},
    project::{
        ProjectError, ProjectInputError, ProjectPhaseError, ProjectRelativePath, ResolutionRequest,
        ResolutionRequestKey, ResolverOutcome, SourceFile, tables::SourceTable,
    },
};

/// Borrowed session state that replaces direct `&Linter` references in the
/// project session, analysis, and resolution chain.
pub struct SessionState<'a> {
    pub(super) analyzer: SemanticAnalyzer<'a>,
    pub(super) artifact_cache: ArtifactCacheHandle,
    catalog: &'a RuleCatalog,
    enabled: &'a [RuleIndex],
    evidence_limit: usize,
    project_limits: ProjectAdmissionLimits,
}

impl<'a> SessionState<'a> {
    pub(crate) fn new(
        environment: &'a Environment,
        limits: &'a AnalysisLimits,
        artifact_cache: ArtifactCacheHandle,
        catalog: &'a RuleCatalog,
        enabled: &'a [RuleIndex],
        evidence_limit: usize,
        project_limits: ProjectAdmissionLimits,
    ) -> Self {
        Self {
            analyzer: SemanticAnalyzer::new(environment, limits),
            artifact_cache,
            catalog,
            enabled,
            evidence_limit,
            project_limits,
        }
    }

    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        ArtifactCacheKey::new(source, self.analyzer.environment(), self.analyzer.limits())
    }
}

struct LocalAnalysisTransition<'borrow, 'state> {
    state: &'borrow SessionState<'state>,
    artifacts: &'borrow mut AnalysisArtifacts,
    requests: &'borrow mut Vec<ResolutionRequest>,
    observer: &'borrow dyn ExecutionObserver,
}

impl LocalAnalysisTransition<'_, '_> {
    fn prepare_pending(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob> {
        if !self.artifacts.needs_analysis(&candidate.path) {
            return None;
        }
        self.prepare_cached(candidate)
    }

    fn prepare_requested(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob> {
        self.prepare_cached(candidate)
    }

    fn prepare_cached(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob> {
        let key = self.state.artifact_fingerprint(&candidate.source);
        if let Some(local) = self.state.artifact_cache.get_local(&candidate.source, &key) {
            #[cfg(test)]
            self.observer.record_cache_hit();
            self.requests
                .extend(self.artifacts.record_local(&candidate.path, local));
            None
        } else {
            #[cfg(test)]
            self.observer.record_cache_miss();
            Some(LocalJob {
                path: candidate.path,
                source: candidate.source,
                key,
            })
        }
    }

    fn analyze_requested(&mut self, candidate: LocalJobCandidate) {
        let Some(job) = self.prepare_requested(candidate) else {
            return;
        };
        let result = self.analyze(&job.source);
        self.complete(LocalJobResult {
            path: job.path,
            key: job.key,
            result,
        });
    }

    fn complete(&mut self, result: LocalJobResult) {
        match result.result {
            Ok(analyzed) => {
                let evicted = self
                    .state
                    .artifact_cache
                    .insert_local(result.key, &analyzed);
                #[cfg(not(test))]
                let _ = evicted;
                #[cfg(test)]
                self.observer.record_cache_insert(evicted);
                self.requests
                    .extend(self.artifacts.record_local(&result.path, analyzed));
            }
            Err(error) => {
                self.artifacts.record_parse_failure(result.path, error);
            }
        }
    }

    fn analyze(&self, source: &SourceFile) -> Result<LocalArtifact, ParseDiagnostic> {
        analyze_with_observer(&self.state.analyzer, source, self.observer)
    }
}

impl LocalJobCallbacks for LocalAnalysisTransition<'_, '_> {
    fn prepare(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob> {
        self.prepare_pending(candidate)
    }

    fn release(&mut self, result: LocalJobResult) {
        self.complete(result);
        self.observer.observe(ExecutionEvent::Merged);
    }

    fn discard(&mut self, _job: LocalJob) {
        self.observer.observe(ExecutionEvent::Merged);
    }
}

pub struct ProjectSession<'a> {
    pub(super) state: SessionState<'a>,
    pub(super) sources: SourceTable,
    artifacts: AnalysisArtifacts,
    executor: ThreadLocalJobExecutor,
}

impl<'a> ProjectSession<'a> {
    /// Start an empty parse-once project session under a canonical root.
    pub(crate) fn new(state: SessionState<'a>) -> Self {
        Self {
            state,
            sources: SourceTable::default(),
            artifacts: AnalysisArtifacts::default(),
            executor: ThreadLocalJobExecutor::new(),
        }
    }

    fn accept_normalized_source(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        self.accept_sources([source])
    }

    fn accept_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
    ) -> Result<(), ProjectInputError> {
        self.sources.admit_all(
            sources,
            self.state.project_limits.max_sources(),
            self.state.project_limits.max_source_bytes(),
        )
    }

    /// Analyze one owned source and return its authored requests.
    pub fn analyze_source(&mut self, source: SourceFile) -> Result<AuthoredRequests, ProjectError> {
        let path = source.path().clone();
        self.accept_normalized_source(source)?;
        Ok(AuthoredRequests::new(self.analyze_source_at_path(&path)?))
    }

    #[cfg(test)]
    fn analyze_source_with_observer(
        &mut self,
        path: impl AsRef<str>,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectPhaseError> {
        let path = ProjectRelativePath::new(path.as_ref())
            .map_err(|_| ProjectPhaseError::InvalidTarget(path.as_ref().to_owned()))?;
        self.analyze_source_at_path_with_observer(&path, observer)
    }

    fn analyze_source_at_path_with_observer(
        &mut self,
        path: &ProjectRelativePath,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectPhaseError> {
        let source = self
            .sources
            .get(path)
            .cloned()
            .ok_or_else(|| ProjectPhaseError::UnknownImporter(path.to_string()))?;
        let mut requests = Vec::new();
        let mut transition = LocalAnalysisTransition {
            state: &self.state,
            artifacts: &mut self.artifacts,
            requests: &mut requests,
            observer,
        };
        let candidate = LocalJobCandidate {
            path: path.clone(),
            source,
        };
        transition.analyze_requested(candidate);
        Ok(requests)
    }

    pub(crate) fn analyze_source_at_path(
        &mut self,
        path: &ProjectRelativePath,
    ) -> Result<Vec<ResolutionRequest>, ProjectPhaseError> {
        self.analyze_source_at_path_with_observer(path, &NoopExecutionObserver)
    }

    #[cfg(test)]
    pub(super) fn analyze_source_counted(
        &mut self,
        path: impl AsRef<str>,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectPhaseError> {
        self.analyze_source_with_observer(path, observer)
    }

    #[cfg(test)]
    pub(super) fn accept_test_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.accept_normalized_source(source)
    }

    /// Analyze all accepted sources using a bounded worker count. Canonical
    /// maps and final request sorting make results independent of worker count
    /// and task completion order.
    fn analyze_pending_sources(
        &mut self,
        worker_count: usize,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        let observer = NoopExecutionObserver;
        Self::analyze_pending_sources_with(
            &self.state,
            &self.sources,
            &mut self.artifacts,
            &mut self.executor,
            worker_count,
            &observer,
        )
    }

    /// Admit and analyze owned sources with bounded local execution.
    pub fn analyze_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        workers: NonZeroUsize,
    ) -> Result<AuthoredRequests, ProjectError> {
        self.accept_sources(sources)?;
        Ok(AuthoredRequests::new(
            self.analyze_pending_sources(workers.get())?,
        ))
    }

    fn analyze_pending_sources_with<E: LocalJobExecutor>(
        state: &SessionState<'a>,
        sources: &SourceTable,
        artifacts: &mut AnalysisArtifacts,
        executor: &mut E,
        worker_count: usize,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        let worker_count = normalize_worker_limit(worker_count);
        let mut requests = Vec::new();
        {
            let mut callbacks = LocalAnalysisTransition {
                state,
                artifacts,
                requests: &mut requests,
                observer,
            };
            let mut candidates =
                sources
                    .in_normalized_path_order()
                    .map(|(path, source)| LocalJobCandidate {
                        path: path.clone(),
                        source: source.clone(),
                    });
            executor
                .execute(
                    &mut candidates,
                    worker_count,
                    &state.analyzer,
                    observer,
                    &mut callbacks,
                )
                .map_err(ProjectError::Execution)?;
        }
        requests.sort_by(|left, right| {
            (left.key(), left.specifier()).cmp(&(right.key(), right.specifier()))
        });
        Ok(requests)
    }

    #[cfg(test)]
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    pub(super) fn analyze_sources_controlled(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        worker_count: usize,
        order: ControlledReleaseOrder,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        self.accept_sources(sources)?;
        let observer = NoopExecutionObserver;
        Self::analyze_pending_sources_with(
            &self.state,
            &self.sources,
            &mut self.artifacts,
            &mut ControlledLocalJobExecutor(order),
            worker_count,
            &observer,
        )
    }

    #[cfg(test)]
    pub(super) fn analyze_sources_counted(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        worker_count: usize,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        self.accept_sources(sources)?;
        Self::analyze_pending_sources_with(
            &self.state,
            &self.sources,
            &mut self.artifacts,
            &mut self.executor,
            worker_count,
            observer,
        )
    }

    /// Consume the collection, validate its authored resolution outcomes, and
    /// link, match, and assemble the project report.
    pub fn finish(
        self,
        outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
    ) -> Result<ProjectAnalysis, ProjectError> {
        self.artifacts.validate_complete(&self.sources)?;

        let (link_input, parse_diagnostics) =
            self.artifacts.into_link_input(&self.sources, outcomes)?;

        Ok(ProjectReportAssembler::link(
            &self.sources,
            link_input,
            parse_diagnostics,
            self.state.analyzer.limits(),
        )
        .assemble(
            self.state.catalog,
            self.state.enabled,
            self.state.evidence_limit,
        ))
    }
}
