//! Deterministic project admission, local analysis, and staging.
//!
//! Phase-state types (`SessionState`, `ProjectCollection`,
//! `LocallyAnalyzedProject`, `ResolvedProject`) live here. The execution
//! runtime and artifact-management helpers are in sibling submodules.

mod artifacts;
pub(super) mod execution;

use std::{collections::BTreeMap, num::NonZeroUsize};

pub use artifacts::{AnalysisArtifacts, AuthoredRequests};
#[cfg(test)]
pub(super) use execution::{
    ControlledLocalJobExecutor, ControlledReleaseOrder, CountingExecutionObserver,
    InvocationCounts, outstanding_job_bound,
};
use execution::{
    ExecutionEvent, ExecutionObserver, LocalJob, LocalJobCallbacks, LocalJobCandidate,
    LocalJobExecutor, LocalJobResult, NoopExecutionObserver, ThreadLocalJobExecutor,
    normalize_worker_limit,
};

use crate::{
    AnalysisLimits, Environment, RuleCatalog,
    analysis::{ArtifactCacheHandle, ArtifactCacheKey, LoweredSource, Lowerer, ResolvedLinkInput},
    api::classification::RuleIndex,
    project::{
        AnalysisReport, ProjectError, ProjectExecutionError, ProjectInputError,
        ProjectRelativePath, ResolutionRequest, ResolutionRequestKey, ResolverOutcome, SourceFile,
        tables::SourceTable,
    },
};

/// Borrowed session state that replaces direct `&Linter` references in the
/// collection, analysis, and resolution chain.
pub struct SessionState<'a> {
    pub(super) lowerer: Lowerer<'a>,
    pub(super) artifact_cache: ArtifactCacheHandle,
    catalog: &'a RuleCatalog,
    enabled: &'a [RuleIndex],
    evidence_limit: usize,
    #[cfg(test)]
    fingerprint_engine_version: &'static str,
    #[cfg(test)]
    fingerprint_normalization: Option<&'static str>,
}

impl<'a> SessionState<'a> {
    pub(crate) fn new(
        environment: &'a Environment,
        limits: &'a AnalysisLimits,
        artifact_cache: ArtifactCacheHandle,
        catalog: &'a RuleCatalog,
        enabled: &'a [RuleIndex],
        evidence_limit: usize,
    ) -> Self {
        Self {
            lowerer: Lowerer::new(environment, limits),
            artifact_cache,
            catalog,
            enabled,
            evidence_limit,
            #[cfg(test)]
            fingerprint_engine_version: env!("CARGO_PKG_VERSION"),
            #[cfg(test)]
            fingerprint_normalization: None,
        }
    }

    #[cfg(test)]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        if self.fingerprint_normalization.is_none()
            && self.fingerprint_engine_version == env!("CARGO_PKG_VERSION")
        {
            return ArtifactCacheKey::new(
                source,
                self.lowerer.environment(),
                self.lowerer.limits(),
            );
        }
        self.fingerprint_normalization.map_or_else(
            || {
                ArtifactCacheKey::for_engine_version(
                    source,
                    self.lowerer.environment(),
                    self.lowerer.limits(),
                    self.fingerprint_engine_version,
                )
            },
            |normalization| {
                ArtifactCacheKey::for_test_inputs(
                    source,
                    self.lowerer.environment(),
                    self.lowerer.limits(),
                    normalization,
                    self.fingerprint_engine_version,
                )
            },
        )
    }

    #[cfg(not(test))]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        ArtifactCacheKey::new(source, self.lowerer.environment(), self.lowerer.limits())
    }
}

pub struct ProjectCollection<'a> {
    pub(super) state: SessionState<'a>,
    pub(super) sources: SourceTable,
    artifacts: AnalysisArtifacts,
}

struct LocalAnalysisTransition<'borrow, 'state> {
    state: &'borrow SessionState<'state>,
    artifacts: &'borrow mut AnalysisArtifacts,
    requests: &'borrow mut Vec<ResolutionRequest>,
    observer: &'borrow dyn ExecutionObserver,
}

impl LocalAnalysisTransition<'_, '_> {
    fn prepare(&mut self, candidate: LocalJobCandidate, skip_completed: bool) -> Option<LocalJob> {
        if skip_completed && !self.artifacts.needs_analysis(&candidate.path) {
            return None;
        }
        let key = self.state.artifact_fingerprint(&candidate.source);
        if let Some(lowered) = self
            .state
            .artifact_cache
            .get_lowered(&candidate.source, &key)
        {
            self.observer.observe(ExecutionEvent::CacheHit);
            self.requests
                .extend(self.artifacts.record_lowered(&candidate.path, lowered));
            None
        } else {
            self.observer.observe(ExecutionEvent::CacheMiss);
            Some(LocalJob {
                path: candidate.path,
                source: candidate.source,
                key,
            })
        }
    }

    fn complete(&mut self, result: LocalJobResult) {
        match result.result {
            Ok(lowered) => {
                artifacts::insert_and_notify(
                    &self.state.artifact_cache,
                    result.key,
                    &lowered,
                    self.observer,
                );
                self.requests
                    .extend(self.artifacts.record_lowered(&result.path, lowered));
            }
            Err(error) => {
                self.artifacts.record_parse_failure(result.path, error);
            }
        }
    }

    fn lower(&self, source: &SourceFile) -> Result<LoweredSource, crate::ParseDiagnostic> {
        self.observer.observe(ExecutionEvent::ParseAttempted);
        self.observer.observe(ExecutionEvent::LowerAttempted);
        self.state.lowerer.lower_source(source)
    }
}

impl LocalJobCallbacks for LocalAnalysisTransition<'_, '_> {
    fn prepare(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob> {
        Self::prepare(self, candidate, true)
    }

    fn release(&mut self, result: LocalJobResult) {
        self.complete(result);
        self.observer.observe(ExecutionEvent::Merged);
    }

    fn discard(&mut self, _job: LocalJob) {
        self.observer.observe(ExecutionEvent::Merged);
    }
}

/// Project state after every admitted source has completed local analysis.
/// The consuming transition prevents adding sources after this point.
pub struct LocallyAnalyzedProject<'a> {
    state: SessionState<'a>,
    sources: SourceTable,
    artifacts: AnalysisArtifacts,
}

/// Project state after the authored resolution table has been validated.
/// Linking and matching are available only from this phase.
pub struct ResolvedProject<'a> {
    state: SessionState<'a>,
    sources: SourceTable,
    link_input: ResolvedLinkInput,
    parse_diagnostics: BTreeMap<ProjectRelativePath, crate::ParseDiagnostic>,
}

impl<'a> ProjectCollection<'a> {
    /// Start an empty parse-once project session under a canonical root.
    pub(crate) fn new(state: SessionState<'a>) -> Self {
        Self {
            state,
            sources: SourceTable::default(),
            artifacts: AnalysisArtifacts::default(),
        }
    }

    fn admit_normalized_source(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        self.sources.insert(source)
    }

    fn admit_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
    ) -> Result<(), ProjectInputError> {
        sources
            .into_iter()
            .try_for_each(|source| self.admit_normalized_source(source))
    }

    /// Analyze one owned source and return its authored requests.
    pub fn analyze_source(&mut self, source: SourceFile) -> Result<AuthoredRequests, ProjectError> {
        let path = source.path().clone();
        self.admit_normalized_source(source)?;
        Ok(AuthoredRequests::new(self.analyze_source_at_path(&path)?))
    }

    #[cfg(test)]
    fn analyze_source_with_observer(
        &mut self,
        path: impl AsRef<str>,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let path = crate::project::input::normalize_relative(path.as_ref())?;
        self.analyze_source_at_path_with_observer(&path, observer)
    }

    fn analyze_source_at_path_with_observer(
        &mut self,
        path: &ProjectRelativePath,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let source = self
            .sources
            .get(path)
            .cloned()
            .ok_or_else(|| ProjectInputError::InvalidPath(path.to_string()))?;
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
        let Some(job) = transition.prepare(candidate, false) else {
            return Ok(requests);
        };
        let result = transition.lower(&job.source);
        transition.complete(LocalJobResult {
            path: job.path,
            key: job.key,
            result,
        });
        Ok(requests)
    }

    pub(crate) fn analyze_source_at_path(
        &mut self,
        path: &ProjectRelativePath,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_at_path_with_observer(path, &NoopExecutionObserver)
    }

    #[cfg(test)]
    pub(super) fn analyze_source_counted(
        &mut self,
        path: impl AsRef<str>,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_with_observer(path, observer)
    }

    #[cfg(test)]
    pub(super) fn admit_test_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.admit_normalized_source(source)
    }

    /// Analyze all admitted sources using a bounded worker count. Canonical
    /// maps and final request sorting make results independent of worker count
    /// and task completion order.
    fn analyze_pending_sources(
        &mut self,
        worker_count: usize,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        let observer = NoopExecutionObserver;
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, &observer)
    }

    /// Admit and analyze owned sources with bounded local execution.
    pub fn analyze_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        workers: NonZeroUsize,
    ) -> Result<AuthoredRequests, ProjectError> {
        self.admit_sources(sources)?;
        Ok(AuthoredRequests::new(
            self.analyze_pending_sources(workers.get())?,
        ))
    }

    fn analyze_pending_sources_with<E: LocalJobExecutor>(
        &mut self,
        worker_count: usize,
        executor: &E,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectError> {
        let worker_count = normalize_worker_limit(worker_count);
        let mut requests = Vec::new();
        {
            let mut callbacks = LocalAnalysisTransition {
                state: &self.state,
                artifacts: &mut self.artifacts,
                requests: &mut requests,
                observer,
            };
            let mut candidates =
                self.sources
                    .in_path_order()
                    .map(|(path, source)| LocalJobCandidate {
                        path: path.clone(),
                        source: source.clone(),
                    });
            executor
                .execute(
                    &mut candidates,
                    worker_count,
                    &self.state.lowerer,
                    observer,
                    &mut callbacks,
                )
                .map_err(|error| ProjectError::Execution(ProjectExecutionError::Local(error)))?;
        }
        requests.sort_by(|left, right| {
            (
                left.importer().as_str(),
                left.kind(),
                &left.range(),
                left.specifier().as_str(),
            )
                .cmp(&(
                    right.importer().as_str(),
                    right.kind(),
                    &right.range(),
                    right.specifier().as_str(),
                ))
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
        self.admit_sources(sources)?;
        let observer = NoopExecutionObserver;
        self.analyze_pending_sources_with(
            worker_count,
            &ControlledLocalJobExecutor(order),
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
        self.admit_sources(sources)?;
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, observer)
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_engine_version(&mut self, version: &'static str) {
        self.state.fingerprint_engine_version = version;
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_normalization(&mut self, normalization: &'static str) {
        self.state.fingerprint_normalization = Some(normalization);
    }

    /// Consume the collection after local analysis and freeze its authored
    /// request set for the resolution phase.
    pub fn finish_local(self) -> Result<LocallyAnalyzedProject<'a>, ProjectError> {
        self.artifacts.validate_complete(&self.sources)?;
        Ok(LocallyAnalyzedProject {
            state: self.state,
            sources: self.sources,
            artifacts: self.artifacts,
        })
    }
}

impl<'a> LocallyAnalyzedProject<'a> {
    /// Validate resolver outcomes against the frozen authored request table
    /// and build the qualified-request-identity table that linking consumes.
    /// One consuming transition into `ResolvedProject`; intermediate module
    /// and request identity state is private to the artifact transition.
    pub fn resolve(
        self,
        outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
    ) -> Result<ResolvedProject<'a>, ProjectError> {
        let Self {
            state,
            sources,
            artifacts,
        } = self;
        let (link_input, parse_diagnostics) = artifacts.into_link_input(&sources, outcomes)?;
        Ok(ResolvedProject {
            state,
            sources,
            link_input,
            parse_diagnostics,
        })
    }
}

impl ResolvedProject<'_> {
    /// Link, match, and assemble the report. This consuming method cannot be
    /// called twice because the resolved project is moved into the pipeline.
    pub fn finish(self) -> Result<AnalysisReport, ProjectError> {
        let (report, _) = self.finish_with_timings()?.into_parts();
        Ok(report)
    }

    pub fn finish_with_timings(self) -> Result<crate::lint::ProjectAnalysis, ProjectError> {
        Ok(crate::finish_report(
            self.state.catalog,
            self.state.enabled,
            self.state.evidence_limit,
            &self.sources,
            self.link_input,
            self.parse_diagnostics,
            self.state.lowerer.limits(),
        ))
    }
}
