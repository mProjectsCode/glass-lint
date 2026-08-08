//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{
        compiler::CompiledRuleRecord,
        rule::{CompiledCatalogError, MatcherBuildError},
    },
};

/// Compile rules into records in deterministic declaration order.
pub(crate) fn compile_records(
    rules: &[(crate::RuleId, Rule)],
) -> Result<Vec<CompiledRuleRecord>, CompiledCatalogError> {
    rules
        .iter()
        .map(|(rule_id, rule)| {
            CompiledRuleRecord::new(rule_id.clone(), rule).map_err(|e| match e {
                MatcherBuildError::QueryCompileError(diagnostic) => {
                    CompiledCatalogError::InvalidQuery {
                        rule_id: rule_id.to_string(),
                        diagnostic,
                    }
                }
                MatcherBuildError::CompilerInvariant(message) => {
                    CompiledCatalogError::CompilerInvariant {
                        rule_id: rule_id.to_string(),
                        diagnostic: message,
                    }
                }
                MatcherBuildError::InvalidPhysicalPlan(message) => {
                    CompiledCatalogError::InvalidPhysicalPlan {
                        rule_id: rule_id.to_string(),
                        diagnostic: message,
                    }
                }
                MatcherBuildError::InvalidModuleSpecifier(message) => {
                    CompiledCatalogError::InvalidMatcher {
                        rule_id: rule_id.to_string(),
                        message,
                    }
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()
}
