//! Matcher-independent analysis of one source module.
//!
//! Local analysis resolves scopes, values, facts, module interfaces, and
//! function effects exactly once. Project linking and rule selection consume
//! this model later without revisiting the AST.

use std::{collections::BTreeMap, sync::Arc};

use facts::SemanticFacts;
use syntax::SymbolCallProvenance;

use super::{facts, flow, module::ModuleInterface, status::AnalysisStatus, syntax};
use crate::project::ModuleId;

#[derive(Clone, Debug)]
pub struct SourceContext {
    pub(crate) path: crate::project::ProjectRelativePath,
    #[allow(dead_code)]
    pub(crate) language: crate::SourceLanguage,
    pub(crate) text: Arc<str>,
    pub(crate) lines: Arc<crate::SourceLineIndex>,
}

impl SourceContext {
    pub(crate) fn new(source: &crate::SourceFile) -> Self {
        Self {
            path: source.path.clone(),
            language: source.language,
            text: Arc::from(source.source.as_str()),
            lines: Arc::new(crate::SourceLineIndex::new(&source.source)),
        }
    }

    pub(crate) fn range(
        &self,
        span: crate::ByteRange,
    ) -> Result<crate::SourceRange, crate::InvalidSourceRange> {
        self.lines.try_range(&self.text, span)
    }
}

/// Private identity of all inputs that can affect local semantic lowering.
/// Rule selection is intentionally absent: artifacts are matcher-independent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactFingerprint {
    source: String,
    language: crate::SourceLanguage,
    normalization_mode: &'static str,
    environment: crate::Environment,
    limits: crate::ResourceLimits,
    engine_version: &'static str,
}

impl ArtifactFingerprint {
    pub fn new(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::ResourceLimits,
    ) -> Self {
        Self::with_engine_version(source, environment, limits, env!("CARGO_PKG_VERSION"))
    }

    fn with_engine_version(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::ResourceLimits,
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
        limits: &crate::ResourceLimits,
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

    #[cfg(test)]
    pub(crate) fn for_engine_version(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::ResourceLimits,
        engine_version: &'static str,
    ) -> Self {
        Self::with_engine_version(source, environment, limits, engine_version)
    }

    #[cfg(test)]
    pub(crate) fn for_test_inputs(
        source: &crate::SourceFile,
        environment: &crate::Environment,
        limits: &crate::ResourceLimits,
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
pub struct CachedArtifact {
    pub semantic: Arc<SemanticArtifact>,
}

/// Small process-local cache used only as an internal proof of key semantics.
/// Parse failures are deliberately not cached; successfully lowered artifacts,
/// including exhausted ones, are safe to reuse because their status is data.
#[derive(Clone, Default)]
pub struct ArtifactCache {
    entries: Vec<(ArtifactFingerprint, CachedArtifact)>,
}

impl ArtifactCache {
    const MAX_ENTRIES: usize = 64;

    pub fn get(&self, key: &ArtifactFingerprint) -> Option<CachedArtifact> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, artifact)| artifact.clone())
    }

    /// Insert or replace an artifact, returning whether the FIFO capacity
    /// policy evicted the oldest distinct key.
    pub fn insert(&mut self, key: ArtifactFingerprint, artifact: CachedArtifact) -> bool {
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

/// The immutable, matcher-independent result of analyzing one source.
#[derive(Debug, Clone)]
pub struct SemanticArtifact {
    /// Canonical facts, occurrence indexes, and module interface.
    facts: SemanticFacts,
    /// Proven origins for locally named exports.
    export_origins: BTreeMap<String, SymbolCallProvenance>,
    /// Matcher-independent function effects for project flow.
    effects: flow::effect::FunctionEffects,
    status: AnalysisStatus,
}

impl SemanticArtifact {
    pub(in crate::analysis) fn from_lowering(
        facts: SemanticFacts,
        export_origins: BTreeMap<String, SymbolCallProvenance>,
        effects: flow::effect::FunctionEffects,
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

    pub(in crate::analysis) fn effects(&self) -> &flow::effect::FunctionEffects {
        &self.effects
    }

    pub(in crate::analysis) fn status(&self) -> &AnalysisStatus {
        &self.status
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.export_origins.get(name)
    }
}

/// Path-specific source context paired with reusable semantic state.
#[derive(Debug, Clone)]
pub struct LocalArtifact {
    source: SourceContext,
    semantic: Arc<SemanticArtifact>,
}

impl LocalArtifact {
    pub(crate) fn new(source: SourceContext, semantic: Arc<SemanticArtifact>) -> Self {
        Self { source, semantic }
    }

    pub(crate) fn source(&self) -> &SourceContext {
        &self.source
    }

    pub(crate) fn interface(&self) -> &ModuleInterface {
        self.semantic.interface()
    }

    pub(in crate::analysis) fn facts(&self) -> &SemanticFacts {
        self.semantic.facts()
    }

    pub(in crate::analysis) fn effects(&self) -> &flow::effect::FunctionEffects {
        self.semantic.effects()
    }

    pub(in crate::analysis) fn status(&self) -> &AnalysisStatus {
        self.semantic.status()
    }

    pub(in crate::analysis) fn export_origin(&self, name: &str) -> Option<&SymbolCallProvenance> {
        self.semantic.export_origin(name)
    }
}

/// A successfully analyzed source together with the data needed to report
/// findings in its original file.
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
        &self.local.source().path
    }

    /// Borrow the source map used for location conversion.
    pub(crate) fn source(&self) -> &SourceContext {
        self.local.source()
    }

    /// Borrow this module's local semantic model.
    pub(crate) fn local(&self) -> &LocalArtifact {
        &self.local
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{LocalArtifact, SemanticArtifact, SourceContext};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn local_artifact_is_send_sync_and_cloneable() {
        assert_send_sync::<LocalArtifact>();
        assert_send_sync::<SemanticArtifact>();
    }

    #[test]
    fn source_context_reuses_one_line_index() {
        let source = crate::SourceFile::new("main.js", "fetch('/');").unwrap();
        let context = SourceContext::new(&source);
        let cloned = context.clone();
        assert!(Arc::ptr_eq(&context.lines, &cloned.lines));
        assert_eq!(Arc::strong_count(&context.lines), 2);
    }
}
