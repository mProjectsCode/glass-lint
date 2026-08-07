//! Rule catalog construction and duplicate-ID validation.

use crate::{
    Rule,
    api::{
        compiler::CompiledRuleRecord,
        rule::{CompiledCatalogError, MatcherBuildError, QueryDiagnostic},
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
                MatcherBuildError::QueryBuildError(qbe) => CompiledCatalogError::InvalidQuery {
                    rule_id: rule_id.to_string(),
                    diagnostic: QueryDiagnostic {
                        code: "query_build_error",
                        message: qbe.to_string(),
                    },
                },
                MatcherBuildError::CompilerInvariant(message) => {
                    CompiledCatalogError::CompilerInvariant {
                        rule_id: rule_id.to_string(),
                        message,
                    }
                }
                MatcherBuildError::InvalidPhysicalPlan(message) => {
                    CompiledCatalogError::InvalidPhysicalPlan {
                        rule_id: rule_id.to_string(),
                        message,
                    }
                }
                _ => CompiledCatalogError::InvalidMatcher {
                    rule_id: rule_id.to_string(),
                    message: e.to_string(),
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()
}
