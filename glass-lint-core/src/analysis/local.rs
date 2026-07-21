//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use facts::SemanticFacts;
use smol_str::SmolStr;
use syntax::SymbolCallProvenance;

use crate::{
    analysis::{
        facts, flow::effect::FunctionEffects, module::ModuleInterface, status::AnalysisStatus,
        syntax,
    },
    project::ModuleId,
};

#[derive(Clone, Debug)]
pub struct LocatedSourceContext {
    pub(crate) path: crate::project::ProjectRelativePath,
    pub(crate) text: Arc<str>,
    pub(crate) lines: Arc<crate::SourceLineIndex>,
}

impl LocatedSourceContext {
    pub(crate) fn new(source: &crate::SourceFile) -> Self {
        Self {
            path: source.path.clone(),
            text: Arc::from(source.source.as_str()),
            lines: Arc::new(crate::SourceLineIndex::new(&source.source)),
        }
    }

    pub(crate) fn range(
        &self,
        span: crate::ByteRange,
    ) -> Result<crate::SourceRange, crate::InvalidSourceBoundary> {
        self.lines.try_range(&self.text, span)
    }
}

/// Private identity of all inputs that can affect local semantic lowering.
/// Rule selection is intentionally absent: artifacts are matcher-independent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactCacheKey {
    source: Arc<str>,
    language: crate::SourceLanguage,
    normalization_mode: &'static str,
    environment: crate::Environment,
    limits: crate::AnalysisLimits,
    engine_version: &'static str,
}

impl ArtifactCacheKey {
    pub fn new(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
    ) -> Self {
        Self::with_engine_version(source, environment, limits, env!("CARGO_PKG_VERSION"))
    }

    fn with_engine_version(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
        engine_version: &'static str,
    ) -> Self {
        let normalization_mode = match source.language {
            crate::SourceLanguage::JavaScript => "swc-js-normalization-v1",
            crate::SourceLanguage::TypeScript => "swc-ts-strip-normalization-v1",
        };
        Self::from_inputs(
            source,
            environment,
            limits,
            normalization_mode,
            engine_version,
        )
    }

    fn from_inputs(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
        normalization_mode: &'static str,
        engine_version: &'static str,
    ) -> Self {
        Self {
            source: Arc::from(source.source.as_str()),
            language: source.language,
            normalization_mode,
            environment: environment.clone(),
            limits: limits.clone(),
            engine_version,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_engine_version(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
        engine_version: &'static str,
    ) -> Self {
        Self::with_engine_version(source, environment, limits, engine_version)
    }

    #[cfg(test)]
    pub(crate) fn for_test_inputs(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
        normalization_mode: &'static str,
        engine_version: &'static str,
    ) -> Self {
        Self::from_inputs(
            source,
            environment,
            limits,
            normalization_mode,
            engine_version,
        )
    }
}

#[derive(Clone)]
pub struct SharedSemanticArtifact {
    pub semantic: Arc<SemanticArtifact>,
}

/// Bounded cache of successfully lowered artifacts owned by a reusable runtime.
/// Parse failures are deliberately not cached; successfully lowered artifacts,
/// including exhausted ones, are safe to reuse because their status is data.
///
/// The cache is a simple FIFO vector with a fixed capacity. Lookups are linear
/// but the cache is small (64 entries) and comparison is by full key equality.
#[derive(Default)]
pub struct ArtifactCache {
    entries: Vec<(ArtifactCacheKey, SharedSemanticArtifact)>,
}

/// Synchronized runtime-owned cache. A poisoned mutex is recovered so an
/// optimization can never make analysis panic.
#[derive(Clone, Default)]
pub struct ArtifactCacheHandle(Arc<Mutex<ArtifactCache>>);

impl ArtifactCacheHandle {
    pub fn get(&self, key: &ArtifactCacheKey) -> Option<SharedSemanticArtifact> {
        let cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.get(key)
    }

    pub fn insert(&self, key: ArtifactCacheKey, artifact: SharedSemanticArtifact) -> bool {
        let mut cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(key, artifact)
    }

    #[cfg(test)]
    pub(crate) const fn capacity() -> usize {
        ArtifactCache::capacity()
    }
}

impl ArtifactCache {
    const MAX_ENTRIES: usize = 64;

    pub fn get(&self, key: &ArtifactCacheKey) -> Option<SharedSemanticArtifact> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, artifact)| artifact.clone())
    }

    /// Insert or replace an artifact, returning whether the FIFO capacity
    /// policy evicted the oldest distinct key.
    pub fn insert(&mut self, key: ArtifactCacheKey, artifact: SharedSemanticArtifact) -> bool {
        if let Some((_, existing)) = self
            .entries
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            *existing = artifact;
            return false;
        }
        let evicted = self.entries.len() >= Self::MAX_ENTRIES;
        if evicted {
            self.entries.remove(0);
        }
        self.entries.push((key, artifact));
        evicted
    }

    #[cfg(test)]
    pub(crate) const fn capacity() -> usize {
        Self::MAX_ENTRIES
    }
}

/// The immutable lowered semantic result of analyzing one source.
#[derive(Debug)]
pub struct SemanticArtifact {
    /// Canonical facts, occurrence indexes, and module interface.
    facts: SemanticFacts,
    /// Proven origins for locally named exports.
    export_origins: BTreeMap<SmolStr, SymbolCallProvenance>,
    /// Matcher-independent function effects for project flow.
    effects: FunctionEffects,
    status: AnalysisStatus,
}

impl SemanticArtifact {
    pub(in crate::analysis) fn from_lowering(
        facts: SemanticFacts,
        export_origins: BTreeMap<SmolStr, SymbolCallProvenance>,
        effects: FunctionEffects,
        status: AnalysisStatus,
    ) -> Self {
        Self {
            facts,
            export_origins,
            effects,
            status,
        }
    }

    /// Borrow the matcher-independent module interface.
    pub(crate) fn interface(&self) -> &ModuleInterface {
        self.facts.interface()
    }

    pub(in crate::analysis) fn facts(&self) -> &SemanticFacts {
        &self.facts
    }

    pub(in crate::analysis) fn effects(&self) -> &FunctionEffects {
        &self.effects
    }

    pub(in crate::analysis) fn status(&self) -> &AnalysisStatus {
        &self.status
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.export_origins.get(name)
    }
}

/// Path-specific report attachment paired with reusable lowered semantic state.
#[derive(Debug, Clone)]
pub struct LocalArtifact {
    source: LocatedSourceContext,
    semantic: Arc<SemanticArtifact>,
}

impl LocalArtifact {
    pub(crate) fn new(source: LocatedSourceContext, semantic: Arc<SemanticArtifact>) -> Self {
        Self { source, semantic }
    }

    pub(crate) fn source_context(&self) -> &LocatedSourceContext {
        &self.source
    }

    pub(crate) fn interface(&self) -> &ModuleInterface {
        self.semantic.interface()
    }

    pub(in crate::analysis) fn facts(&self) -> &SemanticFacts {
        self.semantic.facts()
    }

    pub(in crate::analysis) fn effects(&self) -> &FunctionEffects {
        self.semantic.effects()
    }

    pub(in crate::analysis) fn status(&self) -> &AnalysisStatus {
        self.semantic.status()
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.semantic.export_origin(name)
    }
}

/// A linked project module containing one lowered local artifact and its
/// report-local source attachment.
pub struct ProjectModule {
    /// Stable project-local module identity.
    id: ModuleId,
    /// Immutable local semantic model.
    local: LocalArtifact,
}

impl ProjectModule {
    /// Assemble a linked-project module from a stable identity and local
    /// artifact.
    pub(crate) fn new(id: ModuleId, local: LocalArtifact) -> Self {
        Self { id, local }
    }

    /// Return the stable module identity.
    pub(crate) fn id(&self) -> ModuleId {
        self.id
    }

    /// Return the canonical report/resolution path.
    pub(crate) fn path(&self) -> &crate::project::ProjectRelativePath {
        &self.local.source_context().path
    }

    /// Borrow the source map used for location conversion.
    pub(crate) fn source_context(&self) -> &LocatedSourceContext {
        self.local.source_context()
    }

    /// Borrow this module's local semantic model.
    pub(crate) fn local(&self) -> &LocalArtifact {
        &self.local
    }

    /// Return this module's authored requests with source-qualified keys.
    pub(crate) fn authored_requests(&self) -> Vec<crate::ResolutionRequest> {
        self.local.interface().authored_requests(
            self.path(),
            &self.source_context().lines,
            &self.source_context().text,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn local_artifact_is_send_sync_and_cloneable() {
        assert_send_sync::<LocalArtifact>();
        assert_send_sync::<SemanticArtifact>();
    }

    #[test]
    fn source_context_reuses_one_line_index() {
        let source = crate::SourceFile::new("main.js", "fetch('/');").unwrap();
        let context = LocatedSourceContext::new(&source);
        let cloned = context.clone();
        assert!(Arc::ptr_eq(&context.lines, &cloned.lines));
        assert_eq!(Arc::strong_count(&context.lines), 2);
    }
}
