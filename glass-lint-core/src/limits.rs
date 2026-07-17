//! Validated limits for parsing and semantic analysis.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnalysisLimits {
    #[serde(default = "default_syntax_depth")]
    pub syntax_depth: usize,
    #[serde(default = "default_semantic_operations")]
    pub semantic_operations: usize,
    #[serde(default = "default_effect_operations")]
    pub effect_operations: usize,
    #[serde(default = "default_evidence_items")]
    pub evidence_items: usize,
    #[serde(default = "default_link_operations")]
    pub link_operations: usize,
    #[serde(default = "default_flow_operations")]
    pub flow_operations: usize,
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
    pub fn validate(&self) -> Result<(), String> {
        if self.syntax_depth == 0 {
            return Err("syntax_depth must be positive".into());
        }
        if self.semantic_operations == 0 {
            return Err("semantic_operations must be positive".into());
        }
        if self.effect_operations == 0 {
            return Err("effect_operations must be positive".into());
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisLimits;

    #[test]
    fn every_analysis_limit_rejects_zero() {
        let defaults = AnalysisLimits::default();
        let mut cases = [
            ("syntax_depth", defaults.clone()),
            ("semantic_operations", defaults.clone()),
            ("effect_operations", defaults.clone()),
            ("evidence_items", defaults.clone()),
            ("link_operations", defaults.clone()),
            ("flow_operations", defaults),
        ];
        for (name, limits) in &mut cases {
            match *name {
                "syntax_depth" => limits.syntax_depth = 0,
                "semantic_operations" => limits.semantic_operations = 0,
                "effect_operations" => limits.effect_operations = 0,
                "evidence_items" => limits.evidence_items = 0,
                "link_operations" => limits.link_operations = 0,
                "flow_operations" => limits.flow_operations = 0,
                _ => unreachable!(),
            }
            assert_eq!(limits.validate(), Err(format!("{name} must be positive")));
        }
    }
}
