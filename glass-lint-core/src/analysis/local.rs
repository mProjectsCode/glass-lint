//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use facts::SemanticFacts;
use glass_lint_datastructures::{ByteRange, Fingerprint, InvalidSourceBoundary, SourceRange};
use smol_str::SmolStr;
use syntax::SymbolCallProvenance;

use crate::{
    AnalysisLimits, Environment, SourceLanguage, SourceLineIndex,
    analysis::{
        facts, flow::effect::FunctionEffects, lowering::status::AnalysisStatus,
        module::ModuleInterface, syntax,
    },
    project::{ModuleId, ProjectRelativePath, SourceFile, SourceText},
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

impl From<&AnalysisLimits> for LocalLoweringConfig {
    fn from(limits: &AnalysisLimits) -> Self {
        Self {
            syntax_depth: limits.syntax_depth(),
            semantic_operations: limits.semantic_operations(),
            effect_operations: limits.effect_operations(),
        }
    }
}

// ---- Deterministic hasher for cache fingerprints -------------------------

/// XXH3 hash that is deterministic across processes (fixed seed).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ArtifactFingerprint(u64);

/// Current hash version – bump when the encoding of any fingerprint
/// dimension changes so that cached artifacts from older versions are
/// naturally evicted.
const FINGERPRINT_VERSION: u64 = 3;

impl ArtifactFingerprint {
    /// Versioned deterministic hash of all artifact-affecting inputs.
    /// Rule selection is intentionally excluded.
    fn compute(
        source: &SourceText,
        language: SourceLanguage,
        normalization_mode: &str,
        environment: &Environment,
        limits: &LocalLoweringConfig,
        engine_version: &str,
    ) -> Self {
        let mut fp = Fingerprint::init();
        fp.write(&FINGERPRINT_VERSION.to_le_bytes());
        fp.write(source.as_bytes());
        fp.write(&[match language {
            SourceLanguage::JavaScript => 0u8,
            SourceLanguage::TypeScript => 1u8,
        }]);
        fp.write(normalization_mode.as_bytes());
        fp.write(&[0u8]); // separator
        environment.write_fingerprint_bytes(&mut fp);
        fp.write(&limits.syntax_depth.to_le_bytes());
        fp.write(&limits.semantic_operations.to_le_bytes());
        fp.write(&limits.effect_operations.to_le_bytes());
        fp.write(engine_version.as_bytes());
        Self(fp.into_raw())
    }
}

#[derive(Clone, Debug)]
pub struct LocatedSourceContext {
    path: ProjectRelativePath,
    lines: Arc<SourceLineIndex>,
}

impl LocatedSourceContext {
    pub(crate) fn new(source: &SourceFile) -> Self {
        Self {
            path: source.path().clone(),
            lines: Arc::new(SourceLineIndex::from_text(source.source().clone())),
        }
    }

    pub(crate) fn with_index(path: ProjectRelativePath, lines: Arc<SourceLineIndex>) -> Self {
        Self { path, lines }
    }

    pub(crate) fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub(crate) fn lines(&self) -> &SourceLineIndex {
        &self.lines
    }

    pub(crate) fn clone_lines(&self) -> Arc<SourceLineIndex> {
        Arc::clone(&self.lines)
    }

    pub(crate) fn range(&self, span: ByteRange) -> Result<SourceRange, InvalidSourceBoundary> {
        self.lines.try_range(span)
    }
}

/// Private identity of all inputs that can affect local semantic lowering.
/// Rule selection is intentionally absent: artifacts are matcher-independent.
/// Only local-affecting limits (syntax depth, semantic ops, effect ops) are
/// stored; evidence, link, and flow budgets have no impact on lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactCacheKey {
    source: SourceText,
    language: SourceLanguage,
    normalization_mode: &'static str,
    environment: Environment,
    limits: LocalLoweringConfig,
    engine_version: &'static str,
    fingerprint: ArtifactFingerprint,
}

impl ArtifactCacheKey {
    pub fn new(source: &SourceFile, environment: &Environment, limits: &AnalysisLimits) -> Self {
        Self::with_engine_version(source, environment, limits, env!("CARGO_PKG_VERSION"))
    }

    fn with_engine_version(
        source: &SourceFile,
        environment: &Environment,
        limits: &AnalysisLimits,
        engine_version: &'static str,
    ) -> Self {
        let normalization_mode = match source.language() {
            SourceLanguage::JavaScript => "swc-js-normalization-v1",
            SourceLanguage::TypeScript => "swc-ts-strip-normalization-v1",
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
        source: &SourceFile,
        environment: &Environment,
        limits: &AnalysisLimits,
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
        source: &SourceFile,
        environment: &Environment,
        limits: &AnalysisLimits,
        engine_version: &'static str,
    ) -> Self {
        Self::with_engine_version(source, environment, limits, engine_version)
    }

    #[cfg(test)]
    pub(crate) fn for_test_inputs(
        source: &SourceFile,
        environment: &Environment,
        limits: &AnalysisLimits,
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
    semantic: Arc<SemanticArtifact>,
    source_index: Arc<SourceLineIndex>,
}

impl SharedSemanticArtifact {
    pub(crate) fn from_lowered(lowered: &crate::analysis::lowering::LoweredSource) -> Self {
        let (source, semantic) = lowered.clone().into_parts();
        Self {
            semantic,
            source_index: source.clone_lines(),
        }
    }

    fn lowered_for(&self, source: &SourceFile) -> crate::analysis::lowering::LoweredSource {
        crate::analysis::lowering::LoweredSource::new(
            LocatedSourceContext::with_index(source.path().clone(), Arc::clone(&self.source_index)),
            Arc::clone(&self.semantic),
        )
    }
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
    fn get(&self, key: &ArtifactCacheKey) -> Option<SharedSemanticArtifact> {
        let fp = key.fingerprint();
        let cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.get(fp, key)
    }

    fn insert(&self, key: ArtifactCacheKey, artifact: SharedSemanticArtifact) -> bool {
        let fp = key.fingerprint();
        let mut cache = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(fp, key, artifact)
    }

    /// Reconstruct a cache hit with the current source's path and line index.
    pub(crate) fn get_lowered(
        &self,
        source: &SourceFile,
        key: &ArtifactCacheKey,
    ) -> Option<crate::analysis::lowering::LoweredSource> {
        self.get(key).map(|cached| cached.lowered_for(source))
    }

    /// Cache a lowered artifact while retaining only its reusable semantic
    /// state and source-independent line-index data.
    pub(crate) fn insert_lowered(
        &self,
        key: ArtifactCacheKey,
        lowered: &crate::analysis::lowering::LoweredSource,
    ) -> bool {
        self.insert(key, SharedSemanticArtifact::from_lowered(lowered))
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
    /// Lazily derived function effects for project flow.
    effects: OnceLock<FunctionEffects>,
    effect_limit: usize,
    effects_enabled: bool,
    status: AnalysisStatus,
}

impl SemanticArtifact {
    pub(in crate::analysis) fn from_lowering(
        facts: SemanticFacts,
        export_origins: BTreeMap<SmolStr, SymbolCallProvenance>,
        effect_limit: usize,
        effects_enabled: bool,
        status: AnalysisStatus,
    ) -> Self {
        Self {
            facts,
            export_origins,
            effects: OnceLock::new(),
            effect_limit,
            effects_enabled,
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
        self.effects.get_or_init(|| {
            if self.effects_enabled {
                FunctionEffects::collect(self.facts.stream(), self.effect_limit)
            } else {
                FunctionEffects::default()
            }
        })
    }

    #[cfg(test)]
    pub(in crate::analysis) fn effects_initialized(&self) -> bool {
        self.effects.get().is_some()
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
#[derive(Debug)]
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
    pub(crate) fn path(&self) -> &ProjectRelativePath {
        self.local.source_context().path()
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

    fn test_key(text: &str, version: &'static str) -> ArtifactCacheKey {
        let source = crate::project::SourceFile::new("test.js", text).unwrap();
        let env = crate::Environment::default();
        let limits = crate::AnalysisLimits::default();
        ArtifactCacheKey::for_engine_version(&source, &env, &limits, version)
    }

    #[test]
    fn local_artifact_is_send_sync_and_cloneable() {
        assert_send_sync::<LocalArtifact>();
        assert_send_sync::<SemanticArtifact>();
    }

    #[test]
    fn function_effects_are_derived_only_when_requested() {
        let artifact = SemanticArtifact::from_lowering(
            crate::analysis::facts::SemanticFacts::default(),
            BTreeMap::new(),
            usize::MAX,
            true,
            crate::analysis::lowering::status::AnalysisStatus::default(),
        );
        assert!(!artifact.effects_initialized());
        let _ = artifact.effects();
        assert!(artifact.effects_initialized());
    }

    #[test]
    fn source_context_reuses_one_line_index() {
        let source = crate::project::SourceFile::new("main.js", "fetch('/');").unwrap();
        let context = LocatedSourceContext::new(&source);
        let cloned = context.clone();
        assert!(Arc::ptr_eq(&context.lines, &cloned.lines));
        assert_eq!(Arc::strong_count(&context.lines), 2);
    }

    #[test]
    fn artifact_cache_insert_then_get_hit() {
        let mut cache = ArtifactCache::default();
        let key = test_key("x = 1;", "1.0.0");
        let fp = key.fingerprint();
        let artifact = SharedSemanticArtifact {
            semantic: Arc::new(SemanticArtifact::from_lowering(
                crate::analysis::facts::SemanticFacts::default(),
                BTreeMap::new(),
                usize::MAX,
                true,
                crate::analysis::lowering::status::AnalysisStatus::default(),
            )),
            source_index: Arc::new(SourceLineIndex::new("")),
        };
        assert!(cache.get(fp, &key).is_none());
        cache.insert(fp, key.clone(), artifact);
        let retrieved = cache.get(fp, &key);
        assert!(retrieved.is_some());
    }

    #[test]
    fn artifact_cache_evicts_oldest_when_full() {
        let mut cache = ArtifactCache::default();
        let mut keys = Vec::new();
        for i in 0..ArtifactCache::MAX_ENTRIES + 5 {
            let text = format!("x = {i};");
            let key = test_key(&text, "1.0.0");
            let fp = key.fingerprint();
            let artifact = SharedSemanticArtifact {
                semantic: Arc::new(SemanticArtifact::from_lowering(
                    crate::analysis::facts::SemanticFacts::default(),
                    BTreeMap::new(),
                    usize::MAX,
                    true,
                    crate::analysis::lowering::status::AnalysisStatus::default(),
                )),
                source_index: Arc::new(SourceLineIndex::new("")),
            };
            let evicted = cache.insert(fp, key.clone(), artifact);
            keys.push((fp, key));
            if i >= ArtifactCache::MAX_ENTRIES {
                assert!(evicted, "insert {i} should evict oldest");
            }
        }
        let (oldest_fp, oldest_key) = &keys[0];
        assert!(
            cache.get(*oldest_fp, oldest_key).is_none(),
            "oldest entry should be evicted"
        );
        let (newest_fp, newest_key) = keys.last().unwrap();
        assert!(
            cache.get(*newest_fp, newest_key).is_some(),
            "newest entry should be present"
        );
    }

    #[test]
    fn artifact_cache_replacement_does_not_evict() {
        let mut cache = ArtifactCache::default();
        let key = test_key("x = 1;", "1.0.0");
        let fp = key.fingerprint();
        let artifact_a = SharedSemanticArtifact {
            semantic: Arc::new(SemanticArtifact::from_lowering(
                crate::analysis::facts::SemanticFacts::default(),
                BTreeMap::new(),
                usize::MAX,
                true,
                crate::analysis::lowering::status::AnalysisStatus::default(),
            )),
            source_index: Arc::new(SourceLineIndex::new("")),
        };
        let artifact_b = SharedSemanticArtifact {
            semantic: Arc::new(SemanticArtifact::from_lowering(
                crate::analysis::facts::SemanticFacts::default(),
                BTreeMap::new(),
                usize::MAX,
                true,
                crate::analysis::lowering::status::AnalysisStatus::default(),
            )),
            source_index: Arc::new(SourceLineIndex::new("")),
        };
        cache.insert(fp, key.clone(), artifact_a);
        let evicted = cache.insert(fp, key, artifact_b);
        assert!(!evicted, "replacing exact key should not evict");
    }

    #[test]
    fn artifact_cache_miss_on_different_key() {
        let mut cache = ArtifactCache::default();
        let key_a = test_key("x = 1;", "1.0.0");
        let key_b = test_key("y = 2;", "1.0.0");
        let fp_a = key_a.fingerprint();
        let artifact = SharedSemanticArtifact {
            semantic: Arc::new(SemanticArtifact::from_lowering(
                crate::analysis::facts::SemanticFacts::default(),
                BTreeMap::new(),
                usize::MAX,
                true,
                crate::analysis::lowering::status::AnalysisStatus::default(),
            )),
            source_index: Arc::new(SourceLineIndex::new("")),
        };
        cache.insert(fp_a, key_a, artifact);
        assert!(
            cache.get(key_b.fingerprint(), &key_b).is_none(),
            "different key should not hit"
        );
    }
}
