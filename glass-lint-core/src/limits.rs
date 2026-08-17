//! Validated limits for parsing and semantic analysis.

use std::{fmt, num::NonZeroUsize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Error returned when a validated analysis-limit field is zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisLimitError {
    SyntaxDepth,
    SemanticOperations,
    EffectOperations,
    EvidenceItems,
    LinkOperations,
    FlowOperations,
    TraceNodes,
}

/// Validation failures for the aggregate source-admission policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectAdmissionLimitError {
    MaxSources,
    MaxSourceBytes,
}

impl fmt::Display for ProjectAdmissionLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MaxSources => write!(f, "max_sources must be positive"),
            Self::MaxSourceBytes => write!(f, "max_source_bytes must be positive"),
        }
    }
}

impl std::error::Error for ProjectAdmissionLimitError {}

impl fmt::Display for AnalysisLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SyntaxDepth => write!(f, "syntax_depth must be positive"),
            Self::SemanticOperations => write!(f, "semantic_operations must be positive"),
            Self::EffectOperations => write!(f, "effect_operations must be positive"),
            Self::EvidenceItems => write!(f, "evidence_items must be positive"),
            Self::LinkOperations => write!(f, "link_operations must be positive"),
            Self::FlowOperations => write!(f, "flow_operations must be positive"),
            Self::TraceNodes => write!(f, "trace_nodes must be positive"),
        }
    }
}

impl std::error::Error for AnalysisLimitError {}

/// Validated limits for parser and semantic-analysis bounds.
///
/// Every field is guaranteed positive. The only way to obtain a value is
/// through [`Default`] and the named builder methods, all of which reject
/// zero.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct AnalysisLimits {
    syntax_depth: NonZeroUsize,
    semantic_operations: NonZeroUsize,
    effect_operations: NonZeroUsize,
    evidence_items: NonZeroUsize,
    link_operations: NonZeroUsize,
    flow_operations: NonZeroUsize,
    trace_nodes: NonZeroUsize,
}

/// Validated aggregate bounds for sources retained by a direct project
/// session. Filesystem loaders may apply stricter policies before admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectAdmissionLimits {
    max_sources: NonZeroUsize,
    max_source_bytes: NonZeroUsize,
}

pub const DEFAULT_MAX_PROJECT_SOURCES: usize = 10_000;
pub const DEFAULT_MAX_PROJECT_SOURCE_BYTES: usize = 512 * 1024 * 1024;

impl Default for ProjectAdmissionLimits {
    fn default() -> Self {
        Self {
            max_sources: NonZeroUsize::new(DEFAULT_MAX_PROJECT_SOURCES)
                .expect("default max sources is non-zero"),
            max_source_bytes: NonZeroUsize::new(DEFAULT_MAX_PROJECT_SOURCE_BYTES)
                .expect("default max source bytes is non-zero"),
        }
    }
}

impl ProjectAdmissionLimits {
    pub fn new(
        max_sources: usize,
        max_source_bytes: usize,
    ) -> Result<Self, ProjectAdmissionLimitError> {
        let max_sources =
            NonZeroUsize::new(max_sources).ok_or(ProjectAdmissionLimitError::MaxSources)?;
        let max_source_bytes = NonZeroUsize::new(max_source_bytes)
            .ok_or(ProjectAdmissionLimitError::MaxSourceBytes)?;
        Ok(Self {
            max_sources,
            max_source_bytes,
        })
    }

    pub fn max_sources(&self) -> usize {
        self.max_sources.get()
    }

    pub fn max_source_bytes(&self) -> usize {
        self.max_source_bytes.get()
    }

    pub fn with_max_sources(self, value: usize) -> Result<Self, ProjectAdmissionLimitError> {
        Self::new(value, self.max_source_bytes())
    }

    pub fn with_max_source_bytes(self, value: usize) -> Result<Self, ProjectAdmissionLimitError> {
        Self::new(self.max_sources(), value)
    }
}

pub const DEFAULT_SYNTAX_DEPTH: usize = 512;

const fn default_syntax_depth() -> usize {
    DEFAULT_SYNTAX_DEPTH
}
const fn default_semantic_operations() -> usize {
    1_048_576
}
const fn default_effect_operations() -> usize {
    65_536
}
const fn default_evidence_items() -> usize {
    65_536
}
const fn default_link_operations() -> usize {
    1_000_000
}
const fn default_flow_operations() -> usize {
    262_144
}
const fn default_trace_nodes() -> usize {
    65_536
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            syntax_depth: NonZeroUsize::new(default_syntax_depth())
                .expect("default syntax depth is non-zero"),
            semantic_operations: NonZeroUsize::new(default_semantic_operations())
                .expect("default semantic operations are non-zero"),
            effect_operations: NonZeroUsize::new(default_effect_operations())
                .expect("default effect operations are non-zero"),
            evidence_items: NonZeroUsize::new(default_evidence_items())
                .expect("default evidence items are non-zero"),
            link_operations: NonZeroUsize::new(default_link_operations())
                .expect("default link operations are non-zero"),
            flow_operations: NonZeroUsize::new(default_flow_operations())
                .expect("default flow operations are non-zero"),
            trace_nodes: NonZeroUsize::new(default_trace_nodes())
                .expect("default trace nodes are non-zero"),
        }
    }
}

impl AnalysisLimits {
    fn validated(
        value: usize,
        error: AnalysisLimitError,
    ) -> Result<NonZeroUsize, AnalysisLimitError> {
        NonZeroUsize::new(value).ok_or(error)
    }

    pub fn syntax_depth(&self) -> usize {
        self.syntax_depth.get()
    }

    pub fn semantic_operations(&self) -> usize {
        self.semantic_operations.get()
    }

    pub fn effect_operations(&self) -> usize {
        self.effect_operations.get()
    }

    pub fn evidence_items(&self) -> usize {
        self.evidence_items.get()
    }

    pub fn link_operations(&self) -> usize {
        self.link_operations.get()
    }

    pub fn flow_operations(&self) -> usize {
        self.flow_operations.get()
    }

    pub fn trace_nodes(&self) -> usize {
        self.trace_nodes.get()
    }

    /// Builder-style override, validated (may return an error for zero).
    pub fn with_syntax_depth(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(value, AnalysisLimitError::SyntaxDepth, |this, limit| {
            this.syntax_depth = limit;
        })
    }

    pub fn with_semantic_operations(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(
            value,
            AnalysisLimitError::SemanticOperations,
            |this, limit| {
                this.semantic_operations = limit;
            },
        )
    }

    pub fn with_effect_operations(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(
            value,
            AnalysisLimitError::EffectOperations,
            |this, limit| {
                this.effect_operations = limit;
            },
        )
    }

    pub fn with_evidence_items(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(value, AnalysisLimitError::EvidenceItems, |this, limit| {
            this.evidence_items = limit;
        })
    }

    pub fn with_link_operations(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(value, AnalysisLimitError::LinkOperations, |this, limit| {
            this.link_operations = limit;
        })
    }

    pub fn with_flow_operations(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(value, AnalysisLimitError::FlowOperations, |this, limit| {
            this.flow_operations = limit;
        })
    }

    pub fn with_trace_nodes(self, value: usize) -> Result<Self, AnalysisLimitError> {
        self.with_limit(value, AnalysisLimitError::TraceNodes, |this, limit| {
            this.trace_nodes = limit;
        })
    }

    fn with_limit(
        mut self,
        value: usize,
        error: AnalysisLimitError,
        assign: impl FnOnce(&mut Self, NonZeroUsize),
    ) -> Result<Self, AnalysisLimitError> {
        let limit = Self::validated(value, error)?;
        assign(&mut self, limit);
        Ok(self)
    }
}

/// Manual deserializer that validates every field, rejecting zero.
#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for AnalysisLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Raw DTO matching the JSON shape; serde handles defaults.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            #[serde(default = "default_syntax_depth")]
            syntax_depth: usize,
            #[serde(default = "default_semantic_operations")]
            semantic_operations: usize,
            #[serde(default = "default_effect_operations")]
            effect_operations: usize,
            #[serde(default = "default_evidence_items")]
            evidence_items: usize,
            #[serde(default = "default_link_operations")]
            link_operations: usize,
            #[serde(default = "default_flow_operations")]
            flow_operations: usize,
            #[serde(default = "default_trace_nodes")]
            trace_nodes: usize,
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::default()
            .with_syntax_depth(raw.syntax_depth)
            .and_then(|limits| limits.with_semantic_operations(raw.semantic_operations))
            .and_then(|limits| limits.with_effect_operations(raw.effect_operations))
            .and_then(|limits| limits.with_evidence_items(raw.evidence_items))
            .and_then(|limits| limits.with_link_operations(raw.link_operations))
            .and_then(|limits| limits.with_flow_operations(raw.flow_operations))
            .and_then(|limits| limits.with_trace_nodes(raw.trace_nodes))
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
