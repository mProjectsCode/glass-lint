//! Validated resource limits shared by parsing and semantic analysis.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    #[serde(default = "default_syntax_depth")]
    pub syntax_depth: usize,
    #[serde(default = "default_semantic_operations")]
    pub semantic_operations: usize,
    #[serde(default = "default_evidence_items")]
    pub evidence_items: usize,
    #[serde(default = "default_link_operations")]
    pub link_operations: usize,
    #[serde(default = "default_flow_operations")]
    pub flow_operations: usize,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

const fn default_syntax_depth() -> usize {
    512
}
const fn default_semantic_operations() -> usize {
    1_048_576
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
const fn default_timeout_ms() -> u64 {
    5 * 60 * 1000
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            syntax_depth: default_syntax_depth(),
            semantic_operations: default_semantic_operations(),
            evidence_items: default_evidence_items(),
            link_operations: default_link_operations(),
            flow_operations: default_flow_operations(),
            timeout_ms: default_timeout_ms(),
        }
    }
}

impl ResourceLimits {
    pub fn validate(&self) -> Result<(), String> {
        if self.syntax_depth == 0 {
            return Err("syntax_depth must be positive".into());
        }
        if self.semantic_operations == 0 {
            return Err("semantic_operations must be positive".into());
        }
        if self.evidence_items == 0 {
            return Err("evidence_items must be positive".into());
        }
        if self.link_operations == 0 {
            return Err("link_operations must be positive".into());
        }
        if self.flow_operations == 0 {
            return Err("flow_operations must be positive".into());
        }
        if self.timeout_ms == 0 {
            return Err("timeout_ms must be positive".into());
        }
        Ok(())
    }
}
