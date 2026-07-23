//! Deterministic project admission, local analysis, and staging.
//!
//! Phase-state types (`SessionState`, `ProjectCollection`,
//! `LocallyAnalyzedProject`, `ResolvedProject`) live here. The execution
//! runtime and artifact-management helpers are in sibling submodules.

mod artifacts;
pub(super) mod execution;

use std::{collections::BTreeMap, num::NonZeroUsize};

pub use artifacts::SourceAnalysis;
use artifacts::{AnalysisArtifacts, CacheLookup};
#[cfg(test)]
pub(super) use execution::{
    ControlledLocalJobExecutor, ControlledReleaseOrder, CountingExecutionObserver,
    InvocationCounts, outstanding_job_bound,
};
use execution::{
    ExecutionEvent, ExecutionObserver, LocalJob, LocalJobExecutor, NoopExecutionObserver,
    ThreadLocalJobExecutor, normalize_worker_limit,
};

use crate::{
    AnalysisLimits, Environment, ProjectRelativePath, RuleCatalog,
    analysis::{ArtifactCacheHandle, ArtifactCacheKey, LoweredSource, Lowerer, QualifiedRequestId},
    api::classification::RuleIndex,
    lint::ReportAssembly,
    project::{
        AnalysisReport, ProjectInputError, ResolutionRequest, ResolutionRequestKey,
        ResolverOutcome, SourceFile,
        input::{
            ValidatedProjectInput, normalize_relative, normalize_resolution_key, normalize_result,
            normalize_root,
        },
        tables::{ResolutionTable, SourceTable},
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
        }
    }
}

pub struct ProjectCollection<'a> {
    state: SessionState<'a>,
    pub(super) root: std::path::PathBuf,
    pub(super) sources: SourceTable,
    artifacts: AnalysisArtifacts,
    pub(super) artifact_cache: ArtifactCacheHandle,
    #[cfg(test)]
    fingerprint_engine_version: &'static str,
    #[cfg(test)]
    fingerprint_normalization: Option<&'static str>,
}

/// Project state after every admitted source has completed local analysis.
/// The consuming transition prevents adding sources after this point.
pub struct LocallyAnalyzedProject<'a> {
    state: SessionState<'a>,
    root: std::path::PathBuf,
    sources: SourceTable,
    artifacts: AnalysisArtifacts,
}

/// Project state after the authored resolution table has been validated.
/// Linking and matching are available only from this phase.
pub struct ResolvedProject<'a> {
    state: SessionState<'a>,
    input: ValidatedProjectInput,
    artifacts: AnalysisArtifacts,
}

impl<'a> ProjectCollection<'a> {
    #[cfg(test)]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        if self.fingerprint_normalization.is_none()
            && self.fingerprint_engine_version == env!("CARGO_PKG_VERSION")
        {
            return ArtifactCacheKey::new(
                source,
                self.state.lowerer.environment(),
                self.state.lowerer.limits(),
            );
        }
        self.fingerprint_normalization.map_or_else(
            || {
                ArtifactCacheKey::for_engine_version(
                    source,
                    self.state.lowerer.environment(),
                    self.state.lowerer.limits(),
                    self.fingerprint_engine_version,
                )
            },
            |normalization| {
                ArtifactCacheKey::for_test_inputs(
                    source,
                    self.state.lowerer.environment(),
                    self.state.lowerer.limits(),
                    normalization,
                    self.fingerprint_engine_version,
                )
            },
        )
    }

    #[cfg(not(test))]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        ArtifactCacheKey::new(
            source,
            self.state.lowerer.environment(),
            self.state.lowerer.limits(),
        )
    }

    /// Check the artifact cache for a source, returning either a cached
    /// lowered source or the key needed to lower and cache it.
    fn check_cache(&self, source: &SourceFile, observer: &dyn ExecutionObserver) -> CacheLookup {
        let key = self.artifact_fingerprint(source);
        self.artifact_cache.get(&key).map_or_else(
            || {
                observer.observe(ExecutionEvent::CacheMiss);
                CacheLookup::Miss(key)
            },
            |cached| {
                observer.observe(ExecutionEvent::CacheHit);
                CacheLookup::Hit(artifacts::cached_lowered_source(source, &cached))
            },
        )
    }

    /// Start an empty parse-once project session under a canonical root.
    pub fn new(
        state: SessionState<'a>,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ProjectInputError> {
        let artifact_cache = state.artifact_cache.clone();
        Ok(Self {
            state,
            root: normalize_root(&root.into())?,
            sources: SourceTable::default(),
            artifacts: AnalysisArtifacts::default(),
            artifact_cache,
            #[cfg(test)]
            fingerprint_engine_version: env!("CARGO_PKG_VERSION"),
            #[cfg(test)]
            fingerprint_normalization: None,
        })
    }

    fn admit_normalized_source(&mut self, mut source: SourceFile) -> Result<(), ProjectInputError> {
        source.path = normalize_relative(&source.path)?;
        self.sources.insert(source)
    }

    pub(crate) fn admit_validated_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.sources.insert(source)
    }

    /// Analyze one owned source and return its authored requests.
    pub fn analyze_source(
        &mut self,
        source: SourceFile,
    ) -> Result<SourceAnalysis, ProjectInputError> {
        let path = source.path.clone();
        self.admit_normalized_source(source)?;
        Ok(SourceAnalysis {
            requests: self.analyze_source_at_path(&path)?,
        })
    }

    #[cfg(test)]
    fn analyze_source_with_observer(
        &mut self,
        path: impl AsRef<str>,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let path = normalize_relative(path.as_ref())?;
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
            .ok_or_else(|| ProjectInputError::InvalidPath(path.to_string()))?;
        let lowered = match self.check_cache(source, observer) {
            CacheLookup::Hit(lowered) => lowered,
            CacheLookup::Miss(key) => {
                observer.observe(ExecutionEvent::ParseAttempted);
                observer.observe(ExecutionEvent::LowerAttempted);
                let lowered = match self.state.lowerer.lower_source(source) {
                    Ok(lowered) => lowered,
                    Err(error) => {
                        self.artifacts.record_parse_failure(path.clone(), error);
                        return Ok(Vec::new());
                    }
                };
                artifacts::insert_and_notify(&self.artifact_cache, key, &lowered, observer);
                lowered
            }
        };
        Ok(self.record_lowered(path, lowered))
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

    fn record_lowered(
        &mut self,
        path: &ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        self.artifacts.record_lowered(path, lowered)
    }

    /// Analyze all admitted sources using a bounded worker count. Canonical
    /// maps and final request sorting make results independent of worker count
    /// and task completion order.
    fn analyze_pending_sources(
        &mut self,
        worker_count: usize,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let observer = NoopExecutionObserver;
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, &observer)
    }

    /// Admit and analyze owned sources with bounded local execution.
    pub fn analyze_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        workers: NonZeroUsize,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
        self.analyze_pending_sources(workers.get())
    }

    fn analyze_pending_sources_with<E: LocalJobExecutor>(
        &mut self,
        worker_count: usize,
        executor: &E,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let worker_count = normalize_worker_limit(worker_count);
        let pending: Vec<_> = self
            .sources
            .iter()
            .filter(|(path, _)| {
                !self.artifacts.analyzed.contains_key(*path)
                    && !self.artifacts.parse_diagnostics.contains_key(*path)
            })
            .map(|(path, _)| path.to_owned())
            .collect();
        let mut requests = Vec::new();
        let mut uncached = Vec::new();
        for pending_path in &pending {
            let Some(source) = self.sources.get(pending_path) else {
                continue;
            };
            match self.check_cache(source, observer) {
                CacheLookup::Hit(lowered) => {
                    requests.extend(self.record_lowered(pending_path, lowered));
                }
                CacheLookup::Miss(key) => {
                    uncached.push(LocalJob {
                        path: pending_path.clone(),
                        source: source.clone(),
                        key,
                    });
                }
            }
        }

        let artifact_cache = self.artifact_cache.clone();
        let artifacts = &mut self.artifacts;
        let mut release = |result: execution::LocalJobResult| {
            match result.result {
                Ok(lowered) => {
                    artifacts::insert_and_notify(&artifact_cache, result.key, &lowered, observer);
                    requests.extend(artifacts.record_lowered(&result.path, lowered));
                }
                Err(error) => {
                    artifacts.record_parse_failure(result.path, error);
                }
            }
            observer.observe(ExecutionEvent::Merged);
        };
        executor
            .execute(
                Box::new(uncached.into_iter()),
                worker_count,
                &self.state.lowerer,
                observer,
                &mut release,
            )
            .map_err(ProjectInputError::LocalExecution)?;
        requests.sort_by(|left, right| {
            (
                left.key.importer.as_str(),
                left.key.kind,
                &left.key.range,
                left.request.as_str(),
            )
                .cmp(&(
                    right.key.importer.as_str(),
                    right.key.kind,
                    &right.key.range,
                    right.request.as_str(),
                ))
        });
        Ok(requests)
    }

    #[cfg(test)]
    pub(super) fn analyze_sources_controlled(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        worker_count: usize,
        order: ControlledReleaseOrder,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
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
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, observer)
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_engine_version(&mut self, version: &'static str) {
        self.fingerprint_engine_version = version;
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_normalization(&mut self, normalization: &'static str) {
        self.fingerprint_normalization = Some(normalization);
    }

    /// Consume the collection after local analysis and freeze its authored
    /// request set for the resolution phase.
    pub fn finish_local(self) -> LocallyAnalyzedProject<'a> {
        LocallyAnalyzedProject {
            state: self.state,
            root: self.root,
            sources: self.sources,
            artifacts: self.artifacts,
        }
    }
}

impl<'a> LocallyAnalyzedProject<'a> {
    /// Validate resolver outcomes against the frozen authored request table
    /// and build the qualified-request-identity table that linking consumes.
    pub fn resolve(
        self,
        outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
    ) -> Result<ResolvedProject<'a>, ProjectInputError> {
        let mut resolutions = ResolutionTable::default();
        for (mut key, mut result) in outcomes {
            normalize_resolution_key(&mut key)?;
            if !self.artifacts.authored_requests.contains_key(&key) {
                return Err(ProjectInputError::UnknownRequest(key));
            }
            normalize_result(&mut result)?;
            resolutions.insert(key, result)?;
        }
        let input = ValidatedProjectInput::from_maps(
            self.root,
            self.sources.into_map(),
            resolutions.into_map(),
        );
        let request_ids: BTreeMap<ResolutionRequestKey, QualifiedRequestId> = self
            .artifacts
            .analyzed
            .iter()
            .filter_map(|(path, local)| {
                let module_id = input.module_id(path)?;
                Some((path, local, module_id))
            })
            .flat_map(|(path, local, module_id)| {
                let interface = local.interface();
                let lines = &local.source_context().lines;
                let requests = interface.requests_with_ids(path, lines);
                requests.into_iter().map(move |(req_id, authored)| {
                    (
                        authored.key,
                        QualifiedRequestId {
                            module: module_id,
                            request: req_id,
                        },
                    )
                })
            })
            .collect();
        Ok(ResolvedProject {
            state: self.state,
            input: input.with_request_ids(request_ids),
            artifacts: self.artifacts,
        })
    }
}

impl ResolvedProject<'_> {
    /// Link, match, and assemble the report. This consuming method cannot be
    /// called twice because the resolved project is moved into the pipeline.
    pub fn finish(self) -> Result<AnalysisReport, ProjectInputError> {
        self.finish_with_timings().map(|result| result.report)
    }

    pub fn finish_with_timings(self) -> Result<crate::lint::ProjectAnalysis, ProjectInputError> {
        let assembly = ReportAssembly::new(
            self.state.catalog,
            self.state.enabled,
            self.state.evidence_limit,
        );
        assembly.finish(
            self.input,
            self.artifacts.analyzed,
            self.artifacts.parse_diagnostics,
            self.state.lowerer.limits(),
        )
    }
}
