//! Validated limits for parsing and semantic analysis.

use std::fmt;

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
}

impl fmt::Display for AnalysisLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SyntaxDepth => write!(f, "syntax_depth must be positive"),
            Self::SemanticOperations => write!(f, "semantic_operations must be positive"),
            Self::EffectOperations => write!(f, "effect_operations must be positive"),
            Self::EvidenceItems => write!(f, "evidence_items must be positive"),
            Self::LinkOperations => write!(f, "link_operations must be positive"),
            Self::FlowOperations => write!(f, "flow_operations must be positive"),
        }
    }
}

impl std::error::Error for AnalysisLimitError {}

/// Validated limits for parser and semantic-analysis bounds.
///
/// Every field is guaranteed positive. The only way to obtain a value is
/// through [`Self::new`], [`Default`], or deserialization — all of which
/// reject zero.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AnalysisLimits {
    syntax_depth: usize,
    semantic_operations: usize,
    effect_operations: usize,
    evidence_items: usize,
    link_operations: usize,
    flow_operations: usize,
}

const fn default_syntax_depth() -> usize {
    512
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

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            syntax_depth: default_syntax_depth(),
            semantic_operations: default_semantic_operations(),
            effect_operations: default_effect_operations(),
            evidence_items: default_evidence_items(),
            link_operations: default_link_operations(),
            flow_operations: default_flow_operations(),
        }
    }
}

impl AnalysisLimits {
    /// Validate every field and return a trusted instance.
    pub fn new(
        syntax_depth: usize,
        semantic_operations: usize,
        effect_operations: usize,
        evidence_items: usize,
        link_operations: usize,
        flow_operations: usize,
    ) -> Result<Self, AnalysisLimitError> {
        if syntax_depth == 0 {
            return Err(AnalysisLimitError::SyntaxDepth);
        }
        if semantic_operations == 0 {
            return Err(AnalysisLimitError::SemanticOperations);
        }
        if effect_operations == 0 {
            return Err(AnalysisLimitError::EffectOperations);
        }
        if evidence_items == 0 {
            return Err(AnalysisLimitError::EvidenceItems);
        }
        if link_operations == 0 {
            return Err(AnalysisLimitError::LinkOperations);
        }
        if flow_operations == 0 {
            return Err(AnalysisLimitError::FlowOperations);
        }
        Ok(Self {
            syntax_depth,
            semantic_operations,
            effect_operations,
            evidence_items,
            link_operations,
            flow_operations,
        })
    }

    pub fn syntax_depth(&self) -> usize {
        self.syntax_depth
    }

    pub fn semantic_operations(&self) -> usize {
        self.semantic_operations
    }

    pub fn effect_operations(&self) -> usize {
        self.effect_operations
    }

    pub fn evidence_items(&self) -> usize {
        self.evidence_items
    }

    pub fn link_operations(&self) -> usize {
        self.link_operations
    }

    pub fn flow_operations(&self) -> usize {
        self.flow_operations
    }

    /// Builder-style override, validated (may return an error for zero).
    pub fn with_syntax_depth(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::SyntaxDepth);
        }
        self.syntax_depth = value;
        Ok(self)
    }

    pub fn with_semantic_operations(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::SemanticOperations);
        }
        self.semantic_operations = value;
        Ok(self)
    }

    pub fn with_effect_operations(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::EffectOperations);
        }
        self.effect_operations = value;
        Ok(self)
    }

    pub fn with_evidence_items(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::EvidenceItems);
        }
        self.evidence_items = value;
        Ok(self)
    }

    pub fn with_link_operations(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::LinkOperations);
        }
        self.link_operations = value;
        Ok(self)
    }

    pub fn with_flow_operations(mut self, value: usize) -> Result<Self, AnalysisLimitError> {
        if value == 0 {
            return Err(AnalysisLimitError::FlowOperations);
        }
        self.flow_operations = value;
        Ok(self)
    }

    /// Test-only: set a field directly (caller must ensure positivity).
    #[cfg(test)]
    pub fn set_syntax_depth(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.syntax_depth = value;
    }

    #[cfg(test)]
    pub fn set_semantic_operations(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.semantic_operations = value;
    }

    #[cfg(test)]
    pub fn set_effect_operations(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.effect_operations = value;
    }

    #[cfg(test)]
    pub fn set_evidence_items(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.evidence_items = value;
    }

    #[cfg(test)]
    pub fn set_link_operations(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.link_operations = value;
    }

    #[cfg(test)]
    pub fn set_flow_operations(&mut self, value: usize) {
        assert!(value > 0, "test setter requires positive value");
        self.flow_operations = value;
    }
}

/// Manual deserializer that validates every field, rejecting zero.
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
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::new(
            raw.syntax_depth,
            raw.semantic_operations,
            raw.effect_operations,
            raw.evidence_items,
            raw.link_operations,
            raw.flow_operations,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_analysis_limit_rejects_zero() {
        let defaults = AnalysisLimits::default();
        for (variant, zero_fn) in [
            (
                AnalysisLimitError::SyntaxDepth,
                AnalysisLimits::with_syntax_depth as fn(_, _) -> _,
            ),
            (
                AnalysisLimitError::SemanticOperations,
                AnalysisLimits::with_semantic_operations,
            ),
            (
                AnalysisLimitError::EffectOperations,
                AnalysisLimits::with_effect_operations,
            ),
            (
                AnalysisLimitError::EvidenceItems,
                AnalysisLimits::with_evidence_items,
            ),
            (
                AnalysisLimitError::LinkOperations,
                AnalysisLimits::with_link_operations,
            ),
            (
                AnalysisLimitError::FlowOperations,
                AnalysisLimits::with_flow_operations,
            ),
        ] {
            assert_eq!(zero_fn(defaults.clone(), 0), Err(variant));
        }
    }

    #[test]
    fn constructor_rejects_zero() {
        let ok = AnalysisLimits::new(1, 1, 1, 1, 1, 1);
        assert!(ok.is_ok());
        assert_eq!(
            AnalysisLimits::new(0, 1, 1, 1, 1, 1),
            Err(AnalysisLimitError::SyntaxDepth)
        );
        assert_eq!(
            AnalysisLimits::new(1, 0, 1, 1, 1, 1),
            Err(AnalysisLimitError::SemanticOperations)
        );
        assert_eq!(
            AnalysisLimits::new(1, 1, 0, 1, 1, 1),
            Err(AnalysisLimitError::EffectOperations)
        );
        assert_eq!(
            AnalysisLimits::new(1, 1, 1, 0, 1, 1),
            Err(AnalysisLimitError::EvidenceItems)
        );
        assert_eq!(
            AnalysisLimits::new(1, 1, 1, 1, 0, 1),
            Err(AnalysisLimitError::LinkOperations)
        );
        assert_eq!(
            AnalysisLimits::new(1, 1, 1, 1, 1, 0),
            Err(AnalysisLimitError::FlowOperations)
        );
    }

    #[test]
    fn accessors_return_configured_values() {
        let limits = AnalysisLimits::new(10, 20, 30, 40, 50, 60).unwrap();
        assert_eq!(limits.syntax_depth(), 10);
        assert_eq!(limits.semantic_operations(), 20);
        assert_eq!(limits.effect_operations(), 30);
        assert_eq!(limits.evidence_items(), 40);
        assert_eq!(limits.link_operations(), 50);
        assert_eq!(limits.flow_operations(), 60);
    }

    #[test]
    fn deserialization_rejects_zero() {
        let json = r#"{"syntax_depth":0}"#;
        let result: Result<AnalysisLimits, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialization_accepts_partial_with_defaults() {
        let json = r#"{"syntax_depth":256}"#;
        let limits: AnalysisLimits = serde_json::from_str(json).unwrap();
        assert_eq!(limits.syntax_depth(), 256);
        assert_eq!(limits.semantic_operations(), default_semantic_operations());
    }
}
