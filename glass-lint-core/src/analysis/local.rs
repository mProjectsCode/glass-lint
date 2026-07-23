//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::{
    collections::{BTreeMap, VecDeque},
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

/// Inputs from `AnalysisLimits` that affect local semantic lowering.
/// Evidence, link, and flow budgets are intentionally excluded because
/// they only affect downstream matching and linking.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) struct LocalLoweringConfig {
    syntax_depth: usize,
    semantic_operations: usize,
    effect_operations: usize,
}

impl From<&crate::AnalysisLimits> for LocalLoweringConfig {
    fn from(limits: &crate::AnalysisLimits) -> Self {
        Self {
            syntax_depth: limits.syntax_depth(),
            semantic_operations: limits.semantic_operations(),
            effect_operations: limits.effect_operations(),
        }
    }
}

// ---- Deterministic FNV-1a hasher for cache fingerprints -------------------

/// FNV-1a hash that is deterministic across processes (fixed seed).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ArtifactFingerprint(u64);

/// Current hash version – bump when the encoding of any fingerprint
/// dimension changes so that cached artifacts from older versions are
/// naturally evicted.
const FINGERPRINT_VERSION: u64 = 2;

impl ArtifactFingerprint {
    /// Versioned deterministic hash of all artifact-affecting inputs.
    /// Rule selection is intentionally excluded.
    fn compute(
        source: &crate::SourceText,
        language: crate::SourceLanguage,
        normalization_mode: &str,
        environment: &crate::Environment,
        limits: &LocalLoweringConfig,
        engine_version: &str,
    ) -> Self {
        let mut h = crate::fingerprint::fnv_init();
        crate::fingerprint::fnv_write(&mut h, &FINGERPRINT_VERSION.to_le_bytes());
        crate::fingerprint::fnv_write(&mut h, source.as_bytes());
        crate::fingerprint::fnv_write(
            &mut h,
            &[match language {
                crate::SourceLanguage::JavaScript => 0u8,
                crate::SourceLanguage::TypeScript => 1u8,
            }],
        );
        crate::fingerprint::fnv_write(&mut h, normalization_mode.as_bytes());
        crate::fingerprint::fnv_write(&mut h, &[0u8]); // separator
        environment.write_fingerprint_bytes(&mut h);
        crate::fingerprint::fnv_write(&mut h, &limits.syntax_depth.to_le_bytes());
        crate::fingerprint::fnv_write(&mut h, &limits.semantic_operations.to_le_bytes());
        crate::fingerprint::fnv_write(&mut h, &limits.effect_operations.to_le_bytes());
        crate::fingerprint::fnv_write(&mut h, engine_version.as_bytes());
        Self(h)
    }
}

#[derive(Clone, Debug)]
pub struct LocatedSourceContext {
    pub(crate) path: crate::project::ProjectRelativePath,
    pub(crate) lines: Arc<crate::SourceLineIndex>,
}

impl LocatedSourceContext {
    pub(crate) fn new(source: &crate::SourceFile) -> Self {
        Self {
            path: source.path().clone(),
            lines: Arc::new(crate::SourceLineIndex::from_text(source.source().clone())),
        }
    }

    pub(crate) fn range(
        &self,
        span: crate::ByteRange,
    ) -> Result<crate::SourceRange, crate::InvalidSourceBoundary> {
        self.lines.try_range(span)
    }
}

/// Private identity of all inputs that can affect local semantic lowering.
/// Rule selection is intentionally absent: artifacts are matcher-independent.
/// Only local-affecting limits (syntax depth, semantic ops, effect ops) are
/// stored; evidence, link, and flow budgets have no impact on lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactCacheKey {
    source: crate::SourceText,
    language: crate::SourceLanguage,
    normalization_mode: &'static str,
    environment: crate::Environment,
    limits: LocalLoweringConfig,
    engine_version: &'static str,
    fingerprint: ArtifactFingerprint,
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
        let normalization_mode = match source.language() {
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
        let config = LocalLoweringConfig::from(limits);
        let fingerprint = ArtifactFingerprint::compute(
            source.source(),
            source.language(),
            normalization_mode,
            environment,
            &config,
            engine_version,
        );
        Self {
            source: source.source().clone(),
            language: source.language(),
            normalization_mode,
            environment: environment.clone(),
            limits: config,
            engine_version,
            fingerprint,
        }
    }

    /// Return the pre-computed deterministic fingerprint for this key.
    pub(crate) fn fingerprint(&self) -> ArtifactFingerprint {
        self.fingerprint
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

/// One entry in the artifact cache, retaining the full key for collision
/// verification. A fingerprint match is not a hit until the full key matches.
struct CacheEntry {
    fingerprint: ArtifactFingerprint,
    key: ArtifactCacheKey,
    artifact: SharedSemanticArtifact,
}

/// Bounded FIFO artifact cache. Entries are stored in insertion order in a
/// single `VecDeque`, keeping the structure small enough for linear scan
/// (max 64 entries). No internal index synchronization is required.
#[derive(Default)]
pub struct ArtifactCache {
    entries: VecDeque<CacheEntry>,
}

/// Synchronized runtime-owned cache. A poisoned mutex is recovered so an
/// optimization can never make analysis panic.
#[derive(Clone, Default)]
pub struct ArtifactCacheHandle(Arc<Mutex<ArtifactCache>>);

impl ArtifactCacheHandle {
    /// Look up an artifact by fingerprint + full key verification.
    /// The fingerprint is computed *before* acquiring the lock.
    pub fn get(&self, key: &ArtifactCacheKey) -> Option<SharedSemanticArtifact> {
        let fp = key.fingerprint();
        let cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.get(fp, key)
    }

    pub fn insert(&self, key: ArtifactCacheKey, artifact: SharedSemanticArtifact) -> bool {
        let fp = key.fingerprint();
        let mut cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(fp, key, artifact)
    }

    #[cfg(test)]
    pub(crate) const fn capacity() -> usize {
        ArtifactCache::MAX_ENTRIES
    }
}

impl ArtifactCache {
    const MAX_ENTRIES: usize = 64;

    /// Look up by fingerprint then verify full key. Scans the deque linearly;
    /// at the fixed capacity of 64 entries this is faster than maintaining
    /// separate index structures.
    fn get(
        &self,
        fp: ArtifactFingerprint,
        key: &ArtifactCacheKey,
    ) -> Option<SharedSemanticArtifact> {
        self.entries
            .iter()
            .find(|entry| entry.fingerprint == fp && entry.key == *key)
            .map(|entry| entry.artifact.clone())
    }

    /// Insert or replace an artifact. Returns whether the FIFO policy evicted
    /// the oldest entry. An exact-match replacement does not touch the FIFO
    /// and never counts as eviction.
    fn insert(
        &mut self,
        fp: ArtifactFingerprint,
        key: ArtifactCacheKey,
        artifact: SharedSemanticArtifact,
    ) -> bool {
        // Try to replace an exact existing key first.
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.fingerprint == fp && entry.key == key)
        {
            entry.artifact = artifact;
            return false;
        }
        // New entry: enforce FIFO capacity before inserting.
        let evicted = self.entries.len() >= Self::MAX_ENTRIES;
        if evicted {
            self.entries.pop_front();
        }
        self.entries.push_back(CacheEntry {
            fingerprint: fp,
            key,
            artifact,
        });
        evicted
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
