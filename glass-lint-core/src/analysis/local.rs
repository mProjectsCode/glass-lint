//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
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

// ---- Deterministic FNV-1a hasher for cache fingerprints -------------------

/// FNV-1a hash that is deterministic across processes (fixed seed).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ArtifactFingerprint(u64);

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

fn fnv_write(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(FNV_PRIME);
    }
}

/// Current hash version – bump when the encoding of any fingerprint
/// dimension changes so that cached artifacts from older versions are
/// naturally evicted.
const FINGERPRINT_VERSION: u64 = 1;

impl ArtifactFingerprint {
    /// Versioned deterministic hash of all artifact-affecting inputs.
    /// Rule selection is intentionally excluded.
    pub fn compute(
        source: &crate::SourceText,
        language: crate::SourceLanguage,
        normalization_mode: &str,
        environment: &crate::Environment,
        limits: &crate::AnalysisLimits,
        engine_version: &str,
    ) -> Self {
        let mut h = FNV_OFFSET;
        fnv_write(&mut h, &FINGERPRINT_VERSION.to_le_bytes());
        fnv_write(&mut h, source.as_bytes());
        fnv_write(
            &mut h,
            &[match language {
                crate::SourceLanguage::JavaScript => 0u8,
                crate::SourceLanguage::TypeScript => 1u8,
            }],
        );
        fnv_write(&mut h, normalization_mode.as_bytes());
        fnv_write(&mut h, &[0u8]); // separator
        environment.write_fingerprint_bytes(&mut h);
        fnv_write(&mut h, &limits.syntax_depth().to_le_bytes());
        fnv_write(&mut h, &limits.semantic_operations().to_le_bytes());
        fnv_write(&mut h, &limits.effect_operations().to_le_bytes());
        fnv_write(&mut h, &limits.evidence_items().to_le_bytes());
        fnv_write(&mut h, &limits.link_operations().to_le_bytes());
        fnv_write(&mut h, &limits.flow_operations().to_le_bytes());
        fnv_write(&mut h, engine_version.as_bytes());
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
            path: source.path.clone(),
            lines: Arc::new(crate::SourceLineIndex::from_text(source.source.clone())),
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactCacheKey {
    source: crate::SourceText,
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
            source: source.source.clone(),
            language: source.language,
            normalization_mode,
            environment: environment.clone(),
            limits: limits.clone(),
            engine_version,
        }
    }

    /// Compute the deterministic fingerprint for this key.
    pub(crate) fn fingerprint(&self) -> ArtifactFingerprint {
        ArtifactFingerprint::compute(
            &self.source,
            self.language,
            self.normalization_mode,
            &self.environment,
            &self.limits,
            self.engine_version,
        )
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

/// Bounded FIFO artifact cache indexed by a deterministic fingerprint.
///
/// Lookups compute the fingerprint before taking the lock; under the lock
/// only the matching bucket is inspected. Each bucket holds entries whose
/// fingerprints collide; an exact hit requires the full key to match.
///
/// The FIFO eviction policy is explicit: a `VecDeque` tracks insertion-order
/// fingerprints so that eviction pops the oldest entry without shifting a
/// vector or scanning unrelated buckets.
#[derive(Default)]
pub struct ArtifactCache {
    buckets: HashMap<ArtifactFingerprint, Vec<usize>>,
    entries: HashMap<usize, CacheEntry>,
    fifo: VecDeque<usize>,
    next_id: usize,
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

    /// Look up by fingerprint then verify full key.
    fn get(
        &self,
        fp: ArtifactFingerprint,
        key: &ArtifactCacheKey,
    ) -> Option<SharedSemanticArtifact> {
        self.buckets.get(&fp)?.iter().find_map(|id| {
            let entry = self.entries.get(id)?;
            (entry.key == *key).then(|| entry.artifact.clone())
        })
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
        if let Some(bucket) = self.buckets.get_mut(&fp)
            && let Some(id) = bucket
                .iter()
                .copied()
                .find(|id| self.entries.get(id).is_some_and(|entry| entry.key == key))
        {
            self.entries
                .get_mut(&id)
                .expect("bucket entry exists")
                .artifact = artifact;
            return false;
        }
        // New entry: enforce FIFO capacity before inserting.
        let evicted = if self.fifo.len() >= Self::MAX_ENTRIES {
            if let Some(oldest_id) = self.fifo.pop_front()
                && let Some(entry) = self.entries.remove(&oldest_id)
                && let Some(bucket) = self.buckets.get_mut(&entry.fingerprint)
            {
                bucket.retain(|id| *id != oldest_id);
                if bucket.is_empty() {
                    self.buckets.remove(&entry.fingerprint);
                }
            }
            true
        } else {
            false
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.fifo.push_back(id);
        self.buckets.entry(fp).or_default().push(id);
        self.entries.insert(
            id,
            CacheEntry {
                fingerprint: fp,
                key,
                artifact,
            },
        );
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

    pub(crate) fn authored_requests_with_ids(
        &self,
    ) -> Vec<(
        crate::analysis::module::ModuleRequestId,
        crate::ResolutionRequest,
    )> {
        self.local()
            .interface()
            .requests()
            .filter_map(|request| {
                Some((
                    request.id(),
                    crate::ResolutionRequest {
                        key: crate::ResolutionRequestKey {
                            importer: self.path().clone(),
                            kind: request.kind(),
                            range: self.source_context().range(request.span()).ok()?,
                        },
                        request: request.specifier().to_string(),
                    },
                ))
            })
            .collect()
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
